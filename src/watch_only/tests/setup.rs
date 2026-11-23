//! Published public Trezor-vector account only. Never use this fixture for real funds.
use super::*;
use crate::watch_only::setup::{create, read_export};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
};

pub(super) const PUBLIC_EXPORT: &str = "monoform:1:bitcoin-testnet4:5436d724:tpubDCWivZp6qaqCALCt8MyLqAb3awnWm4hfbBPjdZqirYFXYeZ5YsfbWVaPacULZTGtK1RPBSZ92UWNjnhL4fB9UVrF2FjgW8cgmBjxPBmB4iB:0:0:2d159afbde83ae83f819477437aabcd2299049210fda77d2d236211c5907f718";

fn write_export(path: &Path, bytes: &[u8]) {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .unwrap_or_else(|_| panic!("public export fixture"));
}

fn wait(app: &mut MonoformApp) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.pending.is_some() && std::time::Instant::now() < deadline {
        app.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(app.pending.is_none(), "fixture worker finished");
}

#[test]
fn file_review_accepts_only_the_bounded_canonical_public_record() {
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("public fixture"));
    let path = directory.path().join("export");
    write_export(&path, PUBLIC_EXPORT.as_bytes());
    assert!(matches!(read_export(&path), Ok(record) if record == PUBLIC_EXPORT));
    for invalid in [
        vec![],
        vec![b'x'; 257],
        vec![0xff],
        b"tprv-not-a-public-record".to_vec(),
        format!("{PUBLIC_EXPORT}\n").into_bytes(),
        PUBLIC_EXPORT.replace("5436d724", "5436d725").into_bytes(),
    ] {
        write_export(&path, &invalid);
        assert!(matches!(read_export(&path), Err(Failure::InvalidExport)));
    }
}

#[test]
fn file_review_rejects_links_permissions_directories_and_nonblocking_fifo() {
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("public fixture"));
    let path = directory.path().join("export");
    let link = directory.path().join("link");
    write_export(&path, PUBLIC_EXPORT.as_bytes());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
        .unwrap_or_else(|_| panic!("permissions"));
    assert!(matches!(read_export(&path), Err(Failure::ExportFile)));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|_| panic!("permissions"));
    symlink(&path, &link).unwrap_or_else(|_| panic!("fixture symlink"));
    assert!(matches!(read_export(&link), Err(Failure::ExportFile)));
    fs::remove_file(&link).unwrap_or_else(|_| panic!("remove fixture symlink"));
    fs::hard_link(&path, &link).unwrap_or_else(|_| panic!("fixture hard link"));
    assert!(matches!(read_export(&path), Err(Failure::ExportFile)));
    fs::remove_file(&link).unwrap_or_else(|_| panic!("remove fixture hard link"));
    assert!(
        std::process::Command::new("mkfifo")
            .args(["-m", "600"])
            .arg(&link)
            .status()
            .is_ok_and(|status| status.success()),
        "fixture FIFO"
    );
    for invalid in [
        link.as_path(),
        directory.path(),
        Path::new("relative"),
        Path::new("/a\0b"),
    ] {
        assert!(matches!(read_export(invalid), Err(Failure::ExportFile)));
    }
}

#[test]
fn invalid_review_and_creation_do_not_touch_the_destination() {
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("public fixture"));
    let destination = directory.path().join("untouched");
    let mut app = MonoformApp {
        wallet: destination.to_string_lossy().into_owned(),
        export: directory
            .path()
            .join("missing")
            .to_string_lossy()
            .into_owned(),
        setup: true,
        ..Default::default()
    };
    app.start(&egui::Context::default(), Action::Review);
    wait(&mut app);
    assert!(matches!(app.outcome, Some(Err(Failure::ExportFile))));
    assert!(matches!(
        create(&destination, "invalid"),
        Err(Failure::InvalidExport)
    ));
    assert!(!destination.exists());
}

#[test]
fn creation_requires_a_reviewed_record_and_separate_approval() {
    let mut app = configured();
    app.export = "/public-fixture/export".into();
    assert!(!app.can_start(Action::Review));
    app.setup = true;
    assert!(app.can_start(Action::Review));
    app.consent = true;
    app.approved = true;
    assert!(!app.can_start(Action::Refresh));
    assert!(!app.can_start(Action::Create));
    app.outcome = Some(Ok(Completion::Reviewed(PUBLIC_EXPORT.into())));
    app.approved = false;
    assert!(!app.can_start(Action::Create));
    app.approved = true;
    assert!(app.can_start(Action::Create));
    let (_send, receive) = mpsc::sync_channel(1);
    app.pending = Some(receive);
    assert!(!app.can_start(Action::Create));
    assert!(!app.can_start(Action::Review));
}

#[test]
fn successful_real_review_returns_only_validated_public_text_without_creating() {
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("public fixture"));
    let path = directory.path().join("export");
    write_export(&path, PUBLIC_EXPORT.as_bytes());
    let mut app = MonoformApp {
        wallet: directory
            .path()
            .join("untouched")
            .to_string_lossy()
            .into_owned(),
        export: path.to_string_lossy().into_owned(),
        setup: true,
        approved: true,
        ..Default::default()
    };
    app.start(&egui::Context::default(), Action::Review);
    assert!(!app.approved);
    wait(&mut app);
    assert!(
        matches!(&app.outcome, Some(Ok(Completion::Reviewed(record))) if record == PUBLIC_EXPORT)
    );
    assert!(!app.can_start(Action::Create));
    assert!(!Path::new(&app.wallet).exists());
}

#[test]
fn editing_either_setup_path_discards_review_and_approval() {
    for id in ["account-folder", "export-file"] {
        let context = egui::Context::default();
        let mut app = configured();
        app.setup = true;
        app.export = "/public-fixture/export".into();
        app.approved = true;
        app.outcome = Some(Ok(Completion::Reviewed(PUBLIC_EXPORT.into())));
        let mut first = context.run_ui(Default::default(), |ui| app.content(ui));
        first.textures_delta.clear();
        context.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
        let mut frame = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text("-changed".into())],
                ..Default::default()
            },
            |ui| app.content(ui),
        );
        frame.textures_delta.clear();
        assert!(!app.approved);
        assert!(app.outcome.is_none());
        assert!(!app.can_start(Action::Create));
    }
}

#[test]
fn reviewed_record_is_readable_at_both_window_sizes_without_network_consent() {
    for size in [egui::vec2(360.0, 560.0), egui::vec2(460.0, 720.0)] {
        let context = egui::Context::default();
        let mut app = configured();
        app.setup = true;
        app.outcome = Some(Ok(Completion::Reviewed(PUBLIC_EXPORT.into())));
        let mut frame = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| app.content(ui),
        );
        let rendered = text(&frame.shapes);
        frame.textures_delta.clear();
        assert!(rendered.contains(PUBLIC_EXPORT));
        assert!(rendered.contains("m/84'/1'/0'"));
        assert!(rendered.contains("does not prove ownership"));
        assert!(rendered.contains("Create protected account"));
        assert!(!rendered.contains("Allow this connection"));
        assert!(app.pending.is_none());
        assert!(!app.consent);
    }
}

#[test]
fn creation_success_returns_to_open_without_network_consent_or_cached_balance() {
    let mut app = configured();
    app.setup = true;
    app.export = "/public-fixture/export".into();
    let (send, receive) = mpsc::sync_channel(1);
    app.pending = Some(receive);
    assert!(send.send(Ok(Completion::Created)).is_ok());
    app.poll();
    assert!(!app.setup);
    assert!(!app.consent);
    assert!(app.export.is_empty());
    assert!(matches!(app.outcome, Some(Ok(Completion::Created))));
    assert!(message(&Failure::Create(StorageError::Io)).contains("partial account"));
    assert!(message(&Failure::Create(StorageError::AlreadyExists)).contains("Open that account"));
}

#[test]
#[ignore = "creates and deletes one isolated native OS credential for a published public fixture"]
fn native_setup_round_trip() {
    use bitcoin_hashes::{Hash, sha256};
    use std::os::unix::{ffi::OsStrExt, fs::DirBuilderExt};
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("native fixture"));
    let path = directory.path().join("wallet");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap_or_else(|_| panic!("private fixture"));
    let identity = sha256::Hash::hash(
        fs::canonicalize(&path)
            .unwrap_or_else(|_| panic!("fixture path"))
            .as_os_str()
            .as_bytes(),
    );
    let credential = keyring::default::default_credential_builder()
        .build(
            None,
            "org.monoform.wallet.public-state.v1",
            &identity.to_string(),
        )
        .unwrap_or_else(|_| panic!("fixture credential"));
    let entry = keyring::Entry::new_with_credential(credential);
    assert!(matches!(entry.get_secret(), Err(keyring::Error::NoEntry)));
    let result = (|| {
        let mut public =
            monoform_wallet::BitcoinTestnetAccount::from_public_state(PUBLIC_EXPORT).ok()?;
        public.next_receive_address().ok()?;
        public.next_change_address().ok()?;
        let reviewed = public.encode_public_state();
        let export = directory.path().join("export");
        write_export(&export, reviewed.as_bytes());
        let mut app = MonoformApp {
            wallet: path.to_string_lossy().into_owned(),
            export: export.to_string_lossy().into_owned(),
            setup: true,
            ..Default::default()
        };
        app.start(&egui::Context::default(), Action::Review);
        wait(&mut app);
        // The source changes after review; creation must use the approved snapshot.
        write_export(&export, PUBLIC_EXPORT.as_bytes());
        app.approved = true;
        app.start(&egui::Context::default(), Action::Create);
        wait(&mut app);
        if !matches!(app.outcome, Some(Ok(Completion::Created))) {
            return None;
        }
        let store = PublicStateStore::open(&path).ok()?;
        if store.public_account().ok()?.encode_public_state() != reviewed {
            return None;
        }
        drop(store);
        if !matches!(
            create(&path, PUBLIC_EXPORT),
            Err(Failure::Create(StorageError::AlreadyExists))
        ) {
            return None;
        }
        let store = PublicStateStore::open(&path).ok()?;
        (store.public_account().ok()?.encode_public_state() == reviewed).then_some(())
    })();
    assert!(matches!(
        entry.delete_credential(),
        Ok(()) | Err(keyring::Error::NoEntry)
    ));
    assert!(
        result.is_some(),
        "native setup preserved reviewed counters and existing state"
    );
}
