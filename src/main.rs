#![forbid(unsafe_code)]

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};

const INK: Color32 = Color32::from_rgb(238, 238, 235);
const MUTED: Color32 = Color32::from_rgb(152, 152, 146);
const PANEL: Color32 = Color32::from_rgb(27, 27, 26);
const BORDER: Color32 = Color32::from_rgb(55, 55, 52);

const ASSETS: [Asset; 5] = [
    Asset::new("₿", "Bitcoin", "0.091 840 BTC", "$8,934.10"),
    Asset::new("M", "Monero", "11.42 XMR", "$2,091.44"),
    Asset::new("Ξ", "Ethereum", "0.404 ETH", "$1,340.81"),
    Asset::new("D", "Dai", "300.00 DAI · Ethereum", "$300.00"),
    Asset::new("T", "Tether USD", "173.82 USDT · Ethereum", "$173.82"),
];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 720.0])
            .with_min_inner_size([360.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "Monoform Wallet",
        options,
        Box::new(|creation| Ok(Box::new(MonoformApp::new(creation)))),
    )
}

struct Asset {
    symbol: &'static str,
    name: &'static str,
    balance: &'static str,
    fiat: &'static str,
}

impl Asset {
    const fn new(
        symbol: &'static str,
        name: &'static str,
        balance: &'static str,
        fiat: &'static str,
    ) -> Self {
        Self {
            symbol,
            name,
            balance,
            fiat,
        }
    }
}

#[derive(Clone, Copy)]
enum Detail {
    Receive,
    Send,
    Asset(usize),
}

#[derive(Default)]
struct MonoformApp {
    detail: Option<Detail>,
}

impl MonoformApp {
    fn new(creation: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(13, 13, 12);
        visuals.window_fill = PANEL;
        visuals.faint_bg_color = PANEL;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        visuals.widgets.inactive.bg_fill = PANEL;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 38, 36);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
        visuals.override_text_color = Some(INK);
        creation.egui_ctx.set_visuals(visuals);

        Self::default()
    }

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("MONOFORM").strong().extra_letter_spacing(1.5));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("● Demo · no wallet connected").color(MUTED));
            });
        });
        ui.separator();
    }

    fn balance(&self, ui: &mut egui::Ui) {
        ui.add_space(28.0);
        ui.label(RichText::new("TOTAL BALANCE").small().color(MUTED));
        ui.label(RichText::new("$12,840.17").size(38.0).strong());
        ui.label(RichText::new("Fixed demonstration data").color(MUTED));
        ui.add_space(24.0);
    }

    fn actions(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            if columns[0]
                .add_sized(
                    [columns[0].available_width(), 48.0],
                    egui::Button::new("↓  Receive"),
                )
                .clicked()
            {
                self.detail = Some(Detail::Receive);
            }
            if columns[1]
                .add_sized(
                    [columns[1].available_width(), 48.0],
                    egui::Button::new("↑  Send"),
                )
                .clicked()
            {
                self.detail = Some(Detail::Send);
            }
        });
        ui.add_space(28.0);
    }

    fn asset_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Assets").size(20.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(ASSETS.len().to_string()).color(MUTED));
            });
        });

        for (index, asset) in ASSETS.iter().enumerate() {
            let row = egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(8)
                .inner_margin(14)
                .show(ui, |ui| {
                    ui.set_min_height(44.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(asset.symbol).size(22.0).strong());
                        ui.vertical(|ui| {
                            ui.strong(asset.name);
                            ui.label(RichText::new(asset.balance).small().color(MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(asset.fiat);
                        });
                    });
                });

            if row.response.interact(egui::Sense::click()).clicked() {
                self.detail = Some(Detail::Asset(index));
            }
            ui.add_space(8.0);
        }
    }

    fn detail_window(&mut self, ui: &egui::Ui) {
        let Some(detail) = self.detail else {
            return;
        };

        let (title, body) = match detail {
            Detail::Receive => (
                "Receive · demonstration",
                "A production build will create and verify a complete address locally.",
            ),
            Detail::Send => (
                "Transaction review",
                "Recipient\nFull address\n\nAmount\nAsset and network\n\nFee and final total\nDecoded before signing",
            ),
            Detail::Asset(index) => (
                ASSETS[index].name,
                "Balance\nHistory\nReceive address\n\nOne consistent hierarchy for every chain.",
            ),
        };

        let mut close = false;
        egui::Window::new(title)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.set_min_width(300.0);
                ui.label(body);
                ui.add_space(16.0);
                ui.label(
                    RichText::new(
                        "No keys, signing, storage, or network access exists in this build.",
                    )
                    .small()
                    .color(MUTED),
                );
                ui.add_space(12.0);
                close = ui.button("Done").clicked();
            });

        if close {
            self.detail = None;
        }
    }
}

impl eframe::App for MonoformApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(24))
            .show(ui, |ui| {
                self.header(ui);
                self.balance(ui);
                self.actions(ui);
                self.asset_list(ui);
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Sovereign Money, One form, Three Chains")
                        .small()
                        .color(MUTED),
                );
            });

        self.detail_window(ui);
    }
}
