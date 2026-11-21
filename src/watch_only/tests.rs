//! Synthetic UI state only; no real accounts, cookies, credentials or network node.
use super::*;

fn balance() -> Balance {
    Balance {
        confirmed: 1,
        projected: 2,
        immature: 0,
        height: 102,
        observed_at: SystemTime::now(),
    }
}

fn configured() -> MonoformApp {
    MonoformApp {
        wallet: "/public-fixture/account".into(),
        node: "/public-fixture/node".into(),
        ..Default::default()
    }
}

#[test]
fn explicit_consent_and_bounded_absolute_paths_are_required() {
    let mut app = configured();
    assert!(!app.can_refresh());
    app.start(&egui::Context::default());
    assert!(app.pending.is_none());
    app.consent = true;
    assert!(app.can_refresh());
    for invalid in [
        "".to_owned(),
        "relative/path".into(),
        "~/wallet".into(),
        "/a\0b".into(),
        format!("/{}", "x".repeat(4096)),
    ] {
        app.wallet = invalid;
        assert!(!app.can_refresh());
    }
    assert!(valid_folder("/public fixture/account ")); // Do not silently trim paths.
}

#[test]
fn only_one_job_runs_and_failed_or_disconnected_workers_replace_old_data() {
    let mut app = configured();
    app.consent = true;
    let (send, receive) = mpsc::sync_channel(1);
    app.pending = Some(receive);
    assert!(!app.can_refresh());
    app.start(&egui::Context::default());
    assert!(app.pending.is_some());
    assert!(send.send(Ok(balance())).is_ok());
    app.poll();
    assert!(app.pending.is_none());
    assert!(matches!(app.outcome, Some(Ok(_))));
    let (send, receive) = mpsc::sync_channel(1);
    app.pending = Some(receive);
    assert!(
        send.send(Err(Failure::Refresh(RefreshError::CommittedDeadline)))
            .is_ok()
    );
    app.poll();
    assert!(matches!(
        app.outcome,
        Some(Err(Failure::Refresh(RefreshError::CommittedDeadline)))
    ));
    let (send, receive) = mpsc::sync_channel::<Outcome>(1);
    app.pending = Some(receive);
    drop(send);
    app.poll();
    assert!(matches!(app.outcome, Some(Err(Failure::Worker))));
    assert!(app.pending.is_none());
}

#[test]
fn real_worker_returns_missing_account_error_and_clears_previous_balance() {
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary fixture"));
    let mut app = configured();
    app.wallet = directory
        .path()
        .join("missing-account")
        .to_string_lossy()
        .into_owned();
    app.consent = true;
    app.outcome = Some(Ok(balance()));
    app.start(&egui::Context::default());
    assert!(app.outcome.is_none());
    assert!(app.pending.is_some());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.pending.is_some() && std::time::Instant::now() < deadline {
        app.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(app.pending.is_none());
    assert!(matches!(app.outcome, Some(Err(Failure::Store(_)))));
}

#[test]
fn satoshi_display_is_exact_and_commit_failures_explain_uncertainty() {
    assert_eq!(btc(0), "0.00000000 BTC");
    assert_eq!(btc(1), "0.00000001 BTC");
    assert_eq!(btc(21_000_000 * 100_000_000), "21000000.00000000 BTC");
    assert!(
        message(&Failure::Refresh(RefreshError::Commit(
            StorageError::KeyStore
        )))
        .contains("may have advanced")
    );
    assert!(
        message(&Failure::Refresh(RefreshError::CommittedDeadline)).contains("Counters were saved")
    );
}

fn text(shapes: &[egui::epaint::ClippedShape]) -> String {
    fn visit(shape: &egui::epaint::Shape, output: &mut String) {
        match shape {
            egui::epaint::Shape::Text(t) => {
                output.push_str(t.galley.text());
                output.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, output);
                }
            }
            _ => {}
        }
    }
    let mut output = String::new();
    for shape in shapes {
        visit(&shape.shape, &mut output);
    }
    output
}

#[test]
fn narrow_and_full_size_frames_do_not_auto_connect_or_show_demo_money() {
    for size in [egui::vec2(360.0, 560.0), egui::vec2(460.0, 720.0)] {
        let context = egui::Context::default();
        let mut app = MonoformApp::default();
        let mut frame = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    app.content(ui);
                });
            },
        );
        let rendered = text(&frame.shapes);
        frame.textures_delta.clear(); // Headless test intentionally has no GPU uploader.
        assert!(rendered.contains("MONOFORM"));
        assert!(rendered.contains("Testnet4"));
        assert!(rendered.contains("Allow this connection"));
        assert!(!rendered.contains("12,840"));
        assert!(!rendered.contains("Monero"));
        assert!(app.pending.is_none());
        assert!(!app.consent);
    }
}

#[test]
fn editing_either_folder_revokes_consent_and_discards_old_figures() {
    for id in ["account-folder", "node-folder"] {
        let context = egui::Context::default();
        let mut app = configured();
        app.consent = true;
        app.outcome = Some(Ok(balance()));
        let mut first = context.run_ui(Default::default(), |ui| app.content(ui));
        first.textures_delta.clear();
        context.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
        let mut edited = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text("-changed".into())],
                ..Default::default()
            },
            |ui| app.content(ui),
        );
        edited.textures_delta.clear();
        assert!(!app.consent);
        assert!(app.outcome.is_none());
        assert!(app.pending.is_none());
    }
}

#[test]
fn folder_edits_are_disabled_until_the_current_worker_finishes() {
    let context = egui::Context::default();
    let mut app = configured();
    app.consent = true;
    let mut first = context.run_ui(Default::default(), |ui| app.content(ui));
    first.textures_delta.clear();
    let (send, receive) = mpsc::sync_channel(1);
    app.pending = Some(receive);
    let before = app.node.clone();
    context.memory_mut(|memory| memory.request_focus(egui::Id::new("node-folder")));
    let mut busy = context.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Text("-changed".into())],
            ..Default::default()
        },
        |ui| app.content(ui),
    );
    busy.textures_delta.clear();
    assert_eq!(app.node, before);
    assert!(app.consent);
    assert!(app.pending.is_some());
    assert!(send.send(Ok(balance())).is_ok());
    app.poll();
    assert!(app.pending.is_none());
}
