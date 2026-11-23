//! Only the published public account fixture; no seed, credential or node access.
use super::*;

pub(super) fn received() -> Completion {
    let mut public =
        monoform_wallet::BitcoinTestnetAccount::from_public_state(setup::PUBLIC_EXPORT)
            .unwrap_or_else(|_| panic!("published public account"));
    Completion::Receive(ReceiveAddress {
        address: public
            .next_receive_address()
            .unwrap_or_else(|_| panic!("public address")),
        index: 0,
        anchor_height: 102,
        observed_at: SystemTime::now(),
    })
}

#[test]
fn full_saved_address_network_and_index_render_without_automatic_work() {
    for size in [egui::vec2(360.0, 560.0), egui::vec2(460.0, 720.0)] {
        let context = egui::Context::default();
        let mut app = configured();
        let result = received();
        let address = match &result {
            Completion::Receive(value) => value.address.clone(),
            _ => unreachable!(),
        };
        app.outcome = Some(Ok(result));
        let mut frame = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| app.content(ui),
        );
        let rendered = text(&frame.shapes);
        frame.textures_delta.clear();
        assert!(rendered.contains(&address));
        assert!(rendered.contains("Saved receive address · index 0"));
        assert!(rendered.contains("testnet4 only"));
        assert!(rendered.contains("block 102"));
        assert!(rendered.contains("New receive address"));
        assert!(!rendered.contains("Confirmed total"));
        assert!(app.pending.is_none());
        assert!(!app.consent);
    }
}

#[test]
fn receive_needs_consent_and_idle_open_mode_then_clears_old_address_before_work() {
    let mut app = configured();
    app.outcome = Some(Ok(received()));
    assert!(!app.can_start(Action::Receive));
    app.start(&egui::Context::default(), Action::Receive);
    assert!(app.pending.is_none());
    app.consent = true;
    app.setup = true;
    assert!(!app.can_start(Action::Receive));
    app.setup = false;
    assert!(app.can_start(Action::Receive));
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary fixture"));
    app.wallet = directory
        .path()
        .join("missing")
        .to_string_lossy()
        .into_owned();
    app.start(&egui::Context::default(), Action::Receive);
    assert!(app.outcome.is_none());
    assert!(!app.can_start(Action::Receive));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.pending.is_some() && std::time::Instant::now() < deadline {
        app.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(app.outcome, Some(Err(Failure::Store(_)))));
}

#[test]
fn gap_reorg_and_uncertain_or_late_reservations_have_distinct_static_messages() {
    assert!(message(&Failure::Receive(ReceiveError::GapLimit)).contains("Do not lower"));
    assert!(
        message(&Failure::Receive(ReceiveError::Discovery(
            DiscoveryError::Reorg
        )))
        .contains("chain changed")
    );
    assert!(
        message(&Failure::Receive(ReceiveError::Commit(
            StorageError::KeyStore
        )))
        .contains("may have advanced")
    );
    assert!(
        message(&Failure::Receive(ReceiveError::CommittedDeadline)).contains("counter was saved")
    );
}
