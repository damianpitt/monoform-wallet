//! Watch-only public accounts. The UI never holds an OS key or full observation.
mod setup;
use crate::MUTED;
use eframe::egui::{self, RichText};
use monoform_wallet::{
    core_rpc::{CoreError, CoreTestnet4, DiscoveryError, MempoolError, RefreshError, UtxoError},
    storage::{PublicStateStore, StorageError},
};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, SystemTime},
};

struct Balance {
    confirmed: u64,
    projected: u64,
    immature: u64,
    height: u32,
    observed_at: SystemTime,
}

enum Failure {
    Worker,
    ExportFile,
    InvalidExport,
    Create(StorageError),
    Store(StorageError),
    Node(CoreError),
    Refresh(RefreshError),
}

enum Completion {
    Balance(Balance),
    Reviewed(String),
    Created,
}

#[derive(Clone, Copy)]
enum Action {
    Refresh,
    Review,
    Create,
}

type Outcome = Result<Completion, Failure>;

#[derive(Default)]
pub(super) struct MonoformApp {
    wallet: String,
    node: String,
    export: String,
    setup: bool,
    approved: bool,
    consent: bool,
    pending: Option<Receiver<Outcome>>,
    outcome: Option<Outcome>,
}

fn valid_path(text: &str) -> bool {
    text.len() <= 4096 && Path::new(text).is_absolute() && !text.contains('\0')
}

fn btc(sats: u64) -> String {
    format!("{}.{:08} BTC", sats / 100_000_000, sats % 100_000_000)
}

fn refresh(wallet: &Path, node: &Path) -> Outcome {
    // Construct the non-Send credential/store inside its worker, and drop all keys
    // and detailed observations here. Only amounts, height and time cross to the UI.
    let mut store = PublicStateStore::open(wallet).map_err(Failure::Store)?;
    let core = CoreTestnet4::new(node).map_err(Failure::Node)?;
    let result = core
        .refresh_watch_only(&mut store, Default::default())
        .map_err(Failure::Refresh)?;
    Ok(Completion::Balance(Balance {
        confirmed: result.observation.confirmed.balance.total_sats,
        projected: result.observation.summary.projected_total_sats,
        immature: result.observation.confirmed.balance.immature_coinbase_sats,
        height: result.observation.confirmed.anchor_height,
        observed_at: result.observation.observed_at,
    }))
}

impl MonoformApp {
    fn can_start(&self, action: Action) -> bool {
        if self.pending.is_some() || !valid_path(&self.wallet) {
            return false;
        }
        match action {
            Action::Refresh => !self.setup && self.consent && valid_path(&self.node),
            Action::Review => self.setup && valid_path(&self.export),
            Action::Create => {
                self.setup
                    && self.approved
                    && matches!(self.outcome, Some(Ok(Completion::Reviewed(_))))
            }
        }
    }

    fn start(&mut self, context: &egui::Context, action: Action) {
        if !self.can_start(action) {
            return;
        }
        // Creation consumes the exact reviewed snapshot, never a reread of a changed file.
        let reviewed = match self.outcome.take() {
            Some(Ok(Completion::Reviewed(record))) => Some(record),
            _ => None,
        };
        self.approved = false;
        let (send, receive) = mpsc::sync_channel(1);
        let (wallet, node, export) = (self.wallet.clone(), self.node.clone(), self.export.clone());
        let context = context.clone();
        match thread::Builder::new()
            .name("monoform-watch-only".into())
            .spawn(move || {
                let result = match action {
                    Action::Refresh => refresh(Path::new(&wallet), Path::new(&node)),
                    Action::Review => {
                        setup::read_export(Path::new(&export)).map(Completion::Reviewed)
                    }
                    Action::Create => reviewed
                        .ok_or(Failure::InvalidExport)
                        .and_then(|record| setup::create(Path::new(&wallet), &record)),
                };
                let _ = send.send(result);
                context.request_repaint();
            }) {
            Ok(_) => self.pending = Some(receive),
            Err(_) => self.outcome = Some(Err(Failure::Worker)),
        }
    }

    fn poll(&mut self) {
        match self.pending.as_ref().map(Receiver::try_recv) {
            Some(Ok(outcome)) => {
                self.pending = None;
                if matches!(outcome, Ok(Completion::Created)) {
                    self.setup = false;
                    self.export.clear();
                    self.consent = false;
                }
                self.outcome = Some(outcome);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.pending = None;
                self.outcome = Some(Err(Failure::Worker));
            }
            _ => {}
        }
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        self.poll();
        ui.heading(RichText::new("MONOFORM").extra_letter_spacing(1.5));
        ui.label(RichText::new("Bitcoin · Testnet4 · Watch-only").color(MUTED));
        ui.separator();
        ui.add_space(16.0);
        ui.add_enabled_ui(self.pending.is_none(), |ui| {
            if ui
                .checkbox(&mut self.setup, "Set up a watch-only account")
                .changed()
            {
                self.consent = false;
                self.approved = false;
                self.outcome = None;
            }
        });
        ui.label(if self.setup {
            "Review a public export, then create protected local storage. Setup makes no network requests."
        } else {
            "Open an existing protected account using trusted local folders."
        });
        ui.add_space(12.0);
        ui.add_enabled_ui(self.pending.is_none(), |ui| {
            let label = ui.label("Account folder");
            let wallet_changed = ui.add(egui::TextEdit::singleline(&mut self.wallet)
                .id(egui::Id::new("account-folder"))
                .interactive(self.pending.is_none())
                .hint_text("Full path to your protected account folder")
                .char_limit(4096).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
            ui.add_space(8.0);
            let (label, id, hint, field) = if self.setup {
                ("Public export file", "export-file", "Full path to your Monoform public export", &mut self.export)
            } else {
                ("Bitcoin Core data folder", "node-folder", "Full path to your local Bitcoin Core data", &mut self.node)
            };
            let label = ui.label(label);
            let source_changed = ui.add(egui::TextEdit::singleline(field)
                .id(egui::Id::new(id))
                .interactive(self.pending.is_none())
                .hint_text(hint)
                .char_limit(4096).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
            if wallet_changed || source_changed {
                // Consent and displayed values belong to this exact pair of folders.
                self.consent = false;
                self.approved = false;
                self.outcome = None;
            }
            ui.add_space(12.0);
            if self.setup {
                ui.label("Select only a Monoform public export, never a recovery phrase or private-key file.");
            } else {
                ui.label("Refreshing shares this account’s linked addresses with your local node and saves discovered address counters.");
                if ui.checkbox(&mut self.consent, "Allow this connection and refresh").changed() {
                    self.outcome = None;
                }
            }
        });
        ui.add_space(12.0);
        let (action, label) = if self.setup {
            (Action::Review, "Review public export")
        } else {
            (Action::Refresh, "Refresh")
        };
        if ui
            .add_enabled(self.can_start(action), egui::Button::new(label))
            .clicked()
        {
            self.start(ui.ctx(), action);
        }
        if self.pending.is_some() {
            ui.spinner();
            ui.label("Working… System authorization may be required.");
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        } else {
            if ui.button("Clear screen").clicked() {
                *self = Self::default();
            }
            match &self.outcome {
                Some(Ok(Completion::Reviewed(record))) => {
                    ui.label("Bitcoin testnet4 · BIP84 account 0 · m/84'/1'/0'");
                    ui.label("Compare the full public account record and next receive/change counters with your trusted source. A checksum does not prove ownership or freshness; old counters can reuse addresses.");
                    ui.add(
                        egui::Label::new(RichText::new(record).monospace())
                            .wrap()
                            .selectable(true),
                    );
                    ui.checkbox(
                        &mut self.approved,
                        "I verified this account and its counters.",
                    );
                    if ui
                        .add_enabled(
                            self.can_start(Action::Create),
                            egui::Button::new("Create protected account"),
                        )
                        .clicked()
                    {
                        self.start(ui.ctx(), Action::Create);
                    }
                }
                Some(Ok(Completion::Created)) => {
                    ui.label("Protected watch-only account created. Choose your local node and allow a refresh to discover history. No balance has been observed yet.");
                }
                Some(Ok(Completion::Balance(balance))) => {
                    ui.add_space(16.0);
                    ui.label("Confirmed total");
                    ui.label(RichText::new(btc(balance.confirmed)).size(24.0));
                    ui.label("Including pending changes");
                    ui.strong(btc(balance.projected));
                    if balance.immature != 0 {
                        ui.label(format!(
                            "Mining rewards awaiting maturity: {}",
                            btc(balance.immature)
                        ));
                    }
                    ui.label(format!("Through block {}", balance.height));
                    match SystemTime::now().duration_since(balance.observed_at) {
                        Ok(age) => {
                            ui.label(format!("Observed {} seconds ago", age.as_secs()));
                        }
                        Err(_) => {
                            ui.label("Observation time unavailable");
                        }
                    }
                    ui.label("Scoped totals, including immature coins. Pending transactions may change. These are not spendable balances.");
                    // Repaint the age label only; never initiate background queries.
                    ui.ctx().request_repaint_after(Duration::from_secs(1));
                }
                Some(Err(error)) => {
                    ui.label(message(error));
                }
                None => {
                    ui.label("Enter both full paths, then choose an action.");
                }
            }
        }
        ui.add_space(20.0);
        ui.label(
            RichText::new("Sovereign Money, One form, Three Chains")
                .small()
                .color(MUTED),
        );
    }
}

fn message(error: &Failure) -> &'static str {
    match error {
        Failure::Worker => {
            "The worker stopped. If creation or saving was underway, open the account to recover and verify it."
        }
        Failure::ExportFile => {
            "Cannot read that export. Use a private, user-owned regular file (permissions 0600), with no symlink or hard link."
        }
        Failure::InvalidExport => {
            "Invalid public export. Use the exact single-line Monoform testnet4 public-state format, at most 256 bytes, without a trailing newline."
        }
        Failure::Create(StorageError::AlreadyExists) => {
            "Protected state or credentials already exist. Open that account instead."
        }
        Failure::Create(_) => {
            "Creation was not acknowledged. Check the private account folder and system credential store. A partial account may exist; try opening it to recover before attempting creation again."
        }
        Failure::Store(error) | Failure::Refresh(RefreshError::Load(error)) => match error {
            StorageError::KeyStore => "Unlock your system credential store and try again.",
            StorageError::Busy => "This account is already open in another process.",
            StorageError::InvalidState | StorageError::Rollback => {
                "The protected account could not be verified. Refresh stopped."
            }
            _ => "Cannot open that protected account folder. Check its location and permissions.",
        },
        Failure::Node(error)
        | Failure::Refresh(RefreshError::Discovery(DiscoveryError::Backend(error)))
        | Failure::Refresh(RefreshError::Observation(MempoolError::Backend(error)))
        | Failure::Refresh(RefreshError::Observation(MempoolError::Confirmed(
            UtxoError::Backend(error),
        ))) => match error {
            CoreError::Authentication | CoreError::UnsafeCookie => {
                "Cannot authenticate to Bitcoin Core. Check its data folder and permissions."
            }
            CoreError::WrongChain => "This node is not Bitcoin testnet4.",
            _ => {
                "Bitcoin Core is unavailable or returned an invalid response. Check your node and try again."
            }
        },
        Failure::Refresh(RefreshError::Commit(_)) => {
            "Saving failed and counters may have advanced. Refresh again to recover and verify. No balance was accepted."
        }
        Failure::Refresh(RefreshError::CommittedDeadline) => {
            "Counters were saved, but the refresh took too long. Refresh again to verify the balance."
        }
        Failure::Refresh(RefreshError::SnapshotChanged) => {
            "The chain changed during refresh. Try again."
        }
        Failure::Refresh(RefreshError::Deadline) => {
            "The refresh exceeded its time limit. Try again."
        }
        Failure::Refresh(RefreshError::Discovery(_)) => {
            "History discovery is incomplete. Check node sync, the block filter index and available history; the scan may also exceed its limits."
        }
        Failure::Refresh(RefreshError::Observation(_)) => {
            "A fresh balance is unavailable. Pending activity or node responses may be incomplete or exceed this refresh’s limits."
        }
    }
}

impl eframe::App for MonoformApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(24))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.content(ui));
            });
    }
}

#[cfg(test)]
mod tests;
