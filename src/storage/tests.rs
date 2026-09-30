use super::*;
use crate::OfflineSeed;
use std::{
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    sync::{Arc, Mutex},
};
use tempfile::TempDir;

// Public Trezor vector only; never protect real funds with these words.
const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

#[derive(Clone, Default)]
struct MemoryCredential(Arc<Mutex<MemoryState>>);
#[derive(Default)]
struct MemoryState {
    data: Option<Zeroizing<Vec<u8>>>,
    unavailable: bool,
    fail_write: Option<usize>,
    writes: usize,
}

impl Credential for MemoryCredential {
    fn read(&self) -> Result<Option<Zeroizing<Vec<u8>>>, StorageError> {
        let state = self.0.lock().map_err(|_| StorageError::KeyStore)?;
        if state.unavailable {
            return Err(StorageError::KeyStore);
        }
        Ok(state.data.clone())
    }
    fn write(&self, data: &[u8]) -> Result<(), StorageError> {
        let mut state = self.0.lock().map_err(|_| StorageError::KeyStore)?;
        state.writes += 1;
        if state.unavailable || state.fail_write == Some(state.writes) {
            return Err(StorageError::KeyStore);
        }
        state.data = Some(Zeroizing::new(data.to_vec()));
        Ok(())
    }
}

fn account() -> BitcoinTestnetAccount {
    OfflineSeed::from_phrase(PHRASE.to_owned())
        .unwrap_or_else(|_| panic!("public phrase must parse"))
        .bitcoin_testnet_account()
        .unwrap_or_else(|_| panic!("public account must derive"))
}

fn connected(path: &Path, credential: MemoryCredential) -> Result<PublicStateStore, StorageError> {
    let (directory, lock, identity) = PublicStateStore::lock_directory(path, true)?;
    Ok(PublicStateStore {
        directory,
        _lock: lock,
        identity,
        credential: Box::new(credential),
    })
}

fn fixture() -> (TempDir, PublicStateStore, MemoryCredential) {
    let directory = TempDir::new().unwrap_or_else(|_| panic!("temporary fixture directory"));
    let credential = MemoryCredential::default();
    let store = connected(&directory.path().join("wallet"), credential.clone())
        .unwrap_or_else(|_| panic!("fixture must connect"));
    store
        .initialize(&account())
        .unwrap_or_else(|_| panic!("fixture must initialize"));
    (directory, store, credential)
}

fn write_artifact(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .unwrap_or_else(|_| panic!("fixture file must open"));
    file.write_all(bytes)
        .unwrap_or_else(|_| panic!("fixture file must write"));
    file.sync_all()
        .unwrap_or_else(|_| panic!("fixture file must sync"));
}

fn counters(store: &PublicStateStore) -> (u32, u32) {
    let account = store
        .load()
        .unwrap_or_else(|_| panic!("fixture must load"))
        .0;
    (account.next_receive, account.next_change)
}

#[test]
fn encrypted_round_trip_reserves_before_return_and_reopens() {
    let (directory, mut store, credential) = fixture();
    let mut expected = account();
    assert_eq!(
        store.reserve_receive_address(),
        expected
            .next_receive_address()
            .map_err(|_| StorageError::Derivation)
    );
    assert_eq!(
        store.reserve_change_address(),
        expected
            .next_change_address()
            .map_err(|_| StorageError::Derivation)
    );
    assert_eq!(counters(&store), (1, 1));
    let encrypted = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("fixture read"))
        .unwrap_or_else(|| panic!("fixture file exists"));
    assert!(encrypted.starts_with(HEADER));
    let plaintext = expected.encode_public_state();
    assert!(
        !encrypted
            .windows(plaintext.len())
            .any(|window| window == plaintext.as_bytes())
    );
    assert!(
        !encrypted
            .windows(PHRASE.len())
            .any(|window| window == PHRASE.as_bytes())
    );
    assert_eq!(
        store
            .directory
            .metadata()
            .unwrap_or_else(|_| panic!("fixture metadata"))
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(directory.path().join("wallet").join(STATE))
            .unwrap_or_else(|_| panic!("fixture metadata"))
            .mode()
            & 0o777,
        0o600
    );
    drop(store);
    let mut reopened = connected(&directory.path().join("wallet"), credential)
        .unwrap_or_else(|_| panic!("fixture reopen"));
    assert_eq!(
        reopened.reserve_receive_address(),
        expected
            .next_receive_address()
            .map_err(|_| StorageError::Derivation)
    );
    assert_eq!(
        reopened.reserve_change_address(),
        expected
            .next_change_address()
            .map_err(|_| StorageError::Derivation)
    );
}

#[test]
fn unavailable_or_missing_os_key_never_replaces_state() {
    let (_directory, mut store, credential) = fixture();
    let before = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("fixture read"));
    {
        let mut state = credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("fixture lock"));
        state.unavailable = true;
    }
    assert_eq!(store.reserve_receive_address(), Err(StorageError::KeyStore));
    {
        let mut state = credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("fixture lock"));
        state.unavailable = false;
        state.data = None;
    }
    assert_eq!(store.reserve_change_address(), Err(StorageError::KeyStore));
    assert_eq!(
        store.initialize(&account()),
        Err(StorageError::AlreadyExists)
    );
    assert_eq!(
        store
            .read_file(STATE)
            .unwrap_or_else(|_| panic!("fixture read")),
        before
    );
}

#[test]
fn failed_os_writes_do_not_return_addresses_and_recover_correct_side_of_rename() {
    for after_rename in [false, true] {
        let (_directory, mut store, credential) = fixture();
        let before = store
            .read_file(STATE)
            .unwrap_or_else(|_| panic!("fixture read"));
        {
            let mut state = credential
                .0
                .lock()
                .unwrap_or_else(|_| panic!("fixture lock"));
            state.fail_write = Some(state.writes + if after_rename { 2 } else { 1 });
        }
        assert_eq!(store.reserve_receive_address(), Err(StorageError::KeyStore));
        let after = store
            .read_file(STATE)
            .unwrap_or_else(|_| panic!("fixture read"));
        assert_eq!(before == after, !after_rename);
        assert_eq!(counters(&store), (u32::from(after_rename), 0));
        // Retry is safe: an unreturned index may be retried or conservatively skipped.
        assert!(store.reserve_receive_address().is_ok());
        assert_eq!(counters(&store), (1 + u32::from(after_rename), 0));
    }
}

#[test]
fn interrupted_staging_before_rename_keeps_prior_state() {
    let (directory, store, credential) = fixture();
    let (mut next, mut anchor) = store.load().unwrap_or_else(|_| panic!("fixture load"));
    assert!(next.next_receive_address().is_ok());
    let staged = store
        .seal(&next, &anchor)
        .unwrap_or_else(|_| panic!("fixture seal"));
    write_artifact(&directory.path().join("wallet").join(PENDING), &staged);
    anchor[64..96].copy_from_slice(sha256::Hash::hash(&staged).as_byte_array());
    assert!(credential.write(&anchor).is_ok());
    drop(store);
    let reopened = connected(&directory.path().join("wallet"), credential)
        .unwrap_or_else(|_| panic!("fixture reopen"));
    assert_eq!(counters(&reopened), (0, 0));
}

#[test]
fn interrupted_first_creation_recovers_only_the_anchored_staged_file() {
    let directory = TempDir::new().unwrap_or_else(|_| panic!("temporary fixture directory"));
    let credential = MemoryCredential::default();
    let store = connected(&directory.path().join("wallet"), credential.clone())
        .unwrap_or_else(|_| panic!("fixture connect"));
    let mut anchor = Zeroizing::new(vec![0; 96]);
    anchor[..32].fill(42); // Public test key, never a production randomness source.
    let staged = store
        .seal(&account(), &anchor)
        .unwrap_or_else(|_| panic!("fixture seal"));
    write_artifact(&directory.path().join("wallet").join(PENDING), &staged);
    anchor[64..96].copy_from_slice(sha256::Hash::hash(&staged).as_byte_array());
    assert!(credential.write(&anchor).is_ok());
    drop(store);
    let reopened = connected(&directory.path().join("wallet"), credential)
        .unwrap_or_else(|_| panic!("fixture reopen"));
    assert_eq!(counters(&reopened), (0, 0));
    assert!(!directory.path().join("wallet").join(PENDING).exists());
}

#[test]
fn stale_backup_corruption_truncation_and_oversized_files_fail_closed() {
    let (directory, mut store, _) = fixture();
    let path = directory.path().join("wallet").join(STATE);
    let old = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("fixture read"))
        .unwrap_or_else(|| panic!("fixture file exists"));
    assert!(store.reserve_receive_address().is_ok());
    let current = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("fixture read"))
        .unwrap_or_else(|| panic!("fixture file exists"));
    write_artifact(&path, &old);
    assert!(matches!(store.load(), Err(StorageError::Rollback)));
    for end in [0, 4, 28, 43, current.len() - 1] {
        write_artifact(&path, &current[..end]);
        assert!(matches!(store.load(), Err(StorageError::Rollback)));
    }
    let mut tampered = current.clone();
    tampered[30] ^= 1;
    write_artifact(&path, &tampered);
    assert!(matches!(store.load(), Err(StorageError::Rollback)));
    write_artifact(&path, &vec![0; MAX_RECORD as usize + 1]);
    assert!(matches!(store.load(), Err(StorageError::InvalidState)));
    write_artifact(&path, &current);
    assert_eq!(counters(&store), (1, 0));
}

#[test]
fn aead_rejects_wrong_key_tampering_version_and_directory_identity() {
    let (_directory, store, _) = fixture();
    let (account, anchor) = store.load().unwrap_or_else(|_| panic!("fixture load"));
    let record = store
        .seal(&account, &anchor)
        .unwrap_or_else(|_| panic!("fixture seal"));
    let mut wrong_key = anchor.clone();
    wrong_key[0] ^= 1;
    assert!(matches!(
        store.decode(&record, &wrong_key),
        Err(StorageError::InvalidState)
    ));
    for index in [3, 10, 30, record.len() - 1] {
        let mut changed = record.clone();
        changed[index] ^= 1;
        assert!(matches!(
            store.decode(&changed, &anchor),
            Err(StorageError::InvalidState)
        ));
    }
    let (_another_directory, another, _) = fixture();
    assert!(matches!(
        another.decode(&record, &anchor),
        Err(StorageError::InvalidState)
    ));
}

#[test]
fn unsafe_permissions_links_and_interrupted_temp_write_preserve_old_record() {
    let (directory, mut store, _) = fixture();
    let path = directory.path().join("wallet");
    let current = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("fixture read"));
    // A leftover short encrypted write is discarded; the committed record stays intact.
    write_artifact(&path.join(PENDING), b"MFS");
    assert_eq!(counters(&store), (0, 0));
    assert_eq!(
        store
            .read_file(STATE)
            .unwrap_or_else(|_| panic!("fixture read")),
        current
    );
    assert!(store.reserve_receive_address().is_ok());
    fs::set_permissions(path.join(STATE), fs::Permissions::from_mode(0o644))
        .unwrap_or_else(|_| panic!("fixture permissions"));
    assert!(matches!(store.load(), Err(StorageError::UnsafePath)));
    fs::set_permissions(path.join(STATE), fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|_| panic!("fixture permissions"));
    fs::hard_link(path.join(STATE), path.join("fixture-link"))
        .unwrap_or_else(|_| panic!("fixture hard link"));
    assert!(matches!(store.load(), Err(StorageError::UnsafePath)));
    fs::remove_file(path.join("fixture-link")).unwrap_or_else(|_| panic!("remove fixture link"));
    symlink(STATE, path.join(PENDING)).unwrap_or_else(|_| panic!("fixture symlink"));
    assert_eq!(
        store.reserve_change_address(),
        Err(StorageError::UnsafePath)
    );
}

#[test]
fn directory_and_lock_symlinks_or_insecure_directory_are_rejected() {
    let (directory, store, credential) = fixture();
    let path = directory.path().join("wallet");
    drop(store);
    symlink(&path, directory.path().join("alias")).unwrap_or_else(|_| panic!("fixture symlink"));
    assert!(matches!(
        connected(&directory.path().join("alias"), credential.clone()),
        Err(StorageError::UnsafePath)
    ));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|_| panic!("fixture permissions"));
    assert!(matches!(
        connected(&path, credential.clone()),
        Err(StorageError::UnsafePath)
    ));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|_| panic!("fixture permissions"));
    fs::remove_file(path.join("public-state.lock"))
        .unwrap_or_else(|_| panic!("remove fixture lock"));
    symlink(STATE, path.join("public-state.lock")).unwrap_or_else(|_| panic!("fixture symlink"));
    assert!(matches!(
        connected(&path, credential),
        Err(StorageError::UnsafePath)
    ));
}

#[test]
fn another_process_cannot_acquire_the_store_lock() {
    let (directory, store, credential) = fixture();
    let path = directory.path().join("wallet");
    assert!(matches!(
        connected(&path, credential.clone()),
        Err(StorageError::Busy)
    ));
    let executable = std::env::current_exe().unwrap_or_else(|_| panic!("test executable"));
    let output = std::process::Command::new(executable)
        .args(["--ignored", "--exact", "storage::tests::process_lock_probe"])
        .env("MONOFORM_TEST_LOCK_PATH", &path)
        .output()
        .unwrap_or_else(|_| panic!("fixture process"));
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    drop(store);
    assert!(connected(&path, credential).is_ok());
}

#[test]
#[ignore = "helper invoked only by the inter-process locking test"]
fn process_lock_probe() {
    if let Some(path) = std::env::var_os("MONOFORM_TEST_LOCK_PATH") {
        assert!(matches!(
            PublicStateStore::lock_directory(Path::new(&path), false),
            Err(StorageError::Busy)
        ));
    }
}

#[test]
fn exhausted_cursors_and_malformed_os_records_never_advance() {
    let (_directory, mut store, credential) = fixture();
    let (mut account, mut anchor) = store.load().unwrap_or_else(|_| panic!("fixture load"));
    account.next_receive = bip32::ChildNumber::HARDENED_FLAG;
    assert!(store.commit(&account, &mut anchor).is_ok());
    let before = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("fixture read"));
    assert_eq!(
        store.reserve_receive_address(),
        Err(StorageError::Derivation)
    );
    assert_eq!(
        store
            .read_file(STATE)
            .unwrap_or_else(|_| panic!("fixture read")),
        before
    );
    assert!(credential.write(&[0; 95]).is_ok());
    assert!(matches!(store.load(), Err(StorageError::InvalidState)));
}

#[test]
#[ignore = "creates and deletes one isolated OS credential containing a public fixture's storage key"]
fn native_store_round_trip() {
    let directory = TempDir::new().unwrap_or_else(|_| panic!("temporary OS fixture directory"));
    let path = directory.path().join("wallet");
    DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap_or_else(|_| panic!("OS fixture directory"));
    let identity = sha256::Hash::hash(
        fs::canonicalize(&path)
            .unwrap_or_else(|_| panic!("OS fixture path"))
            .as_os_str()
            .as_bytes(),
    );
    let credential = keyring::default::default_credential_builder()
        .build(None, SERVICE, &identity.to_string())
        .unwrap_or_else(|_| panic!("OS fixture credential"));
    let entry = keyring::Entry::new_with_credential(credential);
    assert!(matches!(entry.get_secret(), Err(keyring::Error::NoEntry)));
    let result = (|| -> Result<(), StorageError> {
        let mut store = PublicStateStore::create(&path, &account())?;
        let received = store.reserve_receive_address()?;
        drop(store);
        let mut reopened = PublicStateStore::open(&path)?;
        assert_eq!(counters(&reopened), (1, 0));
        assert_ne!(received, reopened.reserve_receive_address()?);
        Ok(())
    })();
    // Delete only the credential for this fresh, randomized temporary fixture path.
    assert!(matches!(
        entry.delete_credential(),
        Ok(()) | Err(keyring::Error::NoEntry)
    ));
    assert!(result.is_ok(), "native public-fixture round trip failed");
}
