//! Protected testnet4 public state only: no seed storage, signing or UI integration.
use crate::{BitcoinTestnetAccount, BitcoinTestnetDescriptors};
use bitcoin_hashes::{Hash, sha256};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rustix::fs::{self as unix, AtFlags, Mode, OFlags};
use std::{
    fs::{self, DirBuilder, File},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt},
    },
    path::Path,
};
use zeroize::Zeroizing;

const SERVICE: &str = "org.monoform.wallet.public-state.v1";
const STATE: &str = "public-state.mfs";
const PENDING: &str = "public-state.pending";
const HEADER: &[u8; 4] = b"MFS1";
const MAX_RECORD: u64 = 4 + 24 + 256 + 16;

/// Opaque errors: never expose paths, credential payloads or wallet state.
#[derive(Debug, PartialEq, Eq)]
pub enum StorageError {
    Io,
    UnsafePath,
    Busy,
    KeyStore,
    AlreadyExists,
    InvalidState,
    Rollback,
    Derivation,
}

// Only native credentials are constructible through the public API; mocks are test-only.
trait Credential {
    fn read(&self) -> Result<Option<Zeroizing<Vec<u8>>>, StorageError>;
    fn write(&self, data: &[u8]) -> Result<(), StorageError>;
}

struct OsCredential(keyring::Entry);

impl Credential for OsCredential {
    fn read(&self) -> Result<Option<Zeroizing<Vec<u8>>>, StorageError> {
        match self.0.get_secret() {
            Ok(data) => Ok(Some(Zeroizing::new(data))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(StorageError::KeyStore),
        }
    }
    fn write(&self, data: &[u8]) -> Result<(), StorageError> {
        self.0
            .set_secret(data)
            .map_err(|_| StorageError::KeyStore)?;
        // Read-back checks acknowledgement, not the OS service's power-loss durability.
        if self.read()?.as_deref().map(|value| value.as_slice()) != Some(data) {
            return Err(StorageError::KeyStore);
        }
        Ok(())
    }
}

/// One exclusively locked store. Drop releases the lock; no secrets implement Debug.
pub struct PublicStateStore {
    directory: File,
    _lock: File,
    identity: [u8; 32],
    credential: Box<dyn Credential>,
}

impl PublicStateStore {
    /// Explicit creation only. Never overwrite a file or replace an existing OS key.
    /// Parent directories must already exist in trusted, per-user local storage.
    pub fn create(path: &Path, account: &BitcoinTestnetAccount) -> Result<Self, StorageError> {
        let store = Self::connect(path, true)?;
        store.initialize(account)?;
        Ok(store)
    }

    /// Open and recover a known store; missing OS keys never trigger regeneration.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let store = Self::connect(path, false)?;
        store.load()?;
        Ok(store)
    }

    pub fn descriptors(&self) -> Result<BitcoinTestnetDescriptors, StorageError> {
        self.load()?
            .0
            .descriptors()
            .map_err(|_| StorageError::Derivation)
    }

    /// Return an address only after its next index is durably committed.
    pub fn reserve_receive_address(&mut self) -> Result<String, StorageError> {
        self.reserve(false)
    }

    pub fn reserve_change_address(&mut self) -> Result<String, StorageError> {
        self.reserve(true)
    }

    fn connect(path: &Path, create: bool) -> Result<Self, StorageError> {
        let (directory, lock, identity) = Self::lock_directory(path, create)?;
        // Bypass keyring's globally replaceable default builder and mock fallback.
        let credential = keyring::default::default_credential_builder()
            .build(
                None,
                SERVICE,
                &sha256::Hash::from_byte_array(identity).to_string(),
            )
            .map_err(|_| StorageError::KeyStore)?;
        Ok(Self {
            directory,
            _lock: lock,
            identity,
            credential: Box::new(OsCredential(keyring::Entry::new_with_credential(
                credential,
            ))),
        })
    }

    fn lock_directory(path: &Path, create: bool) -> Result<(File, File, [u8; 32]), StorageError> {
        if !path.is_absolute() {
            return Err(StorageError::UnsafePath);
        }
        if create {
            match DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {
                    // Persist the directory entry too, not just the files inside it.
                    File::open(path.parent().ok_or(StorageError::UnsafePath)?)?.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(_) => return Err(StorageError::Io),
            }
        }
        let directory = File::from(
            unix::openat(
                unix::CWD,
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| StorageError::UnsafePath)?,
        );
        Self::check_permissions(&directory, true)?;
        // Subsequent I/O is directory-relative: replacing path components cannot redirect it.
        let lock = local_file(
            &directory,
            "public-state.lock",
            OFlags::RDWR | OFlags::CREATE,
        )
        .map_err(|_| StorageError::UnsafePath)?;
        Self::check_permissions(&lock, false)?;
        match lock.try_lock() {
            Ok(()) => (),
            Err(std::fs::TryLockError::WouldBlock) => return Err(StorageError::Busy),
            Err(_) => return Err(StorageError::Io),
        }
        let canonical = fs::canonicalize(path)?;
        let identity = sha256::Hash::hash(canonical.as_os_str().as_bytes()).to_byte_array();
        Ok((directory, lock, identity))
    }

    fn check_permissions(file: &File, directory: bool) -> Result<(), StorageError> {
        let meta = file.metadata()?;
        let valid_kind = if directory {
            meta.is_dir()
        } else {
            meta.is_file() && meta.nlink() == 1
        };
        let mode = if directory { 0o700 } else { 0o600 };
        if !valid_kind
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o7777 != mode
        {
            return Err(StorageError::UnsafePath);
        }
        Ok(())
    }

    fn read_file(&self, name: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let file = match local_file(&self.directory, name, OFlags::RDONLY) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(_) => return Err(StorageError::UnsafePath),
        };
        Self::check_permissions(&file, false)?;
        let mut record = Vec::new();
        file.take(MAX_RECORD + 1).read_to_end(&mut record)?;
        if record.len() as u64 > MAX_RECORD {
            return Err(StorageError::InvalidState);
        }
        Ok(Some(record))
    }

    fn initialize(&self, account: &BitcoinTestnetAccount) -> Result<(), StorageError> {
        if self.read_file(STATE)?.is_some() || self.credential.read()?.is_some() {
            return Err(StorageError::AlreadyExists);
        }
        // OS record: key[0..32], committed ciphertext hash[32..64], pending hash[64..96].
        let mut anchor = Zeroizing::new(vec![0; 96]);
        getrandom::fill(&mut anchor[..32]).map_err(|_| StorageError::KeyStore)?;
        self.commit(account, &mut anchor)
    }

    fn aad(&self) -> Vec<u8> {
        [HEADER.as_slice(), &self.identity].concat()
    }

    fn seal(
        &self,
        account: &BitcoinTestnetAccount,
        anchor: &[u8],
    ) -> Result<Vec<u8>, StorageError> {
        let mut nonce = [0; 24];
        getrandom::fill(&mut nonce).map_err(|_| StorageError::KeyStore)?;
        let plaintext = Zeroizing::new(account.encode_public_state());
        let cipher = XChaCha20Poly1305::new_from_slice(&anchor[..32])
            .map_err(|_| StorageError::InvalidState)?;
        let encrypted = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: &self.aad(),
                },
            )
            .map_err(|_| StorageError::InvalidState)?;
        Ok([HEADER.as_slice(), &nonce, &encrypted].concat())
    }

    fn decode(&self, record: &[u8], anchor: &[u8]) -> Result<BitcoinTestnetAccount, StorageError> {
        if record.len() < 44 || !record.starts_with(HEADER) {
            return Err(StorageError::InvalidState);
        }
        let cipher = XChaCha20Poly1305::new_from_slice(&anchor[..32])
            .map_err(|_| StorageError::InvalidState)?;
        let nonce: [u8; 24] = record[4..28]
            .try_into()
            .map_err(|_| StorageError::InvalidState)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &record[28..],
                        aad: &self.aad(),
                    },
                )
                .map_err(|_| StorageError::InvalidState)?,
        );
        let text = std::str::from_utf8(&plaintext).map_err(|_| StorageError::InvalidState)?;
        BitcoinTestnetAccount::from_public_state(text).map_err(|_| StorageError::InvalidState)
    }

    fn load(&self) -> Result<(BitcoinTestnetAccount, Zeroizing<Vec<u8>>), StorageError> {
        let mut anchor = self.credential.read()?.ok_or(StorageError::KeyStore)?;
        if anchor.len() != 96 {
            return Err(StorageError::InvalidState);
        }
        let mut record = self.read_file(STATE)?;
        if record.is_none() {
            // Interrupted first creation may have a staged file but no final file yet.
            if anchor[32..64] != [0; 32] || anchor[64..96] == [0; 32] {
                return Err(StorageError::Rollback);
            }
            record = self.read_file(PENDING)?;
            let staged = record.as_ref().ok_or(StorageError::Rollback)?;
            if sha256::Hash::hash(staged).as_byte_array() != &anchor[64..96] {
                return Err(StorageError::Rollback);
            }
            self.decode(staged, &anchor)?;
            unix::renameat(&self.directory, PENDING, &self.directory, STATE)
                .map_err(|_| StorageError::Io)?;
        }
        let record = record.ok_or(StorageError::InvalidState)?;
        let digest = sha256::Hash::hash(&record).to_byte_array();
        if anchor[32..64] != digest && anchor[64..96] != digest {
            return Err(StorageError::Rollback);
        }
        let account = self.decode(&record, &anchor)?;
        if anchor[64..96] != [0; 32] {
            // Before rename: keep the old state. After rename: keep the new state.
            // Nothing was returned before finalization; either can safely skip/retry an index.
            self.sync_state()?;
            anchor[32..64].copy_from_slice(&digest);
            anchor[64..96].fill(0);
            self.credential.write(&anchor)?;
        }
        Ok((account, anchor))
    }

    fn sync_state(&self) -> Result<(), StorageError> {
        let file =
            local_file(&self.directory, STATE, OFlags::RDONLY).map_err(|_| StorageError::Io)?;
        file.sync_all()?;
        self.directory.sync_all()?;
        Ok(())
    }

    fn commit(
        &self,
        account: &BitcoinTestnetAccount,
        anchor: &mut [u8],
    ) -> Result<(), StorageError> {
        // A prior aborted write may leave this one exact, private, bounded artifact.
        if self.read_file(PENDING)?.is_some() {
            unix::unlinkat(&self.directory, PENDING, AtFlags::empty())
                .map_err(|_| StorageError::Io)?;
        }
        let record = self.seal(account, anchor)?;
        let mut file = local_file(
            &self.directory,
            PENDING,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        )
        .map_err(|_| StorageError::Io)?;
        file.write_all(&record)?;
        file.sync_all()?;
        // Make the staged filename durable before its hash is accepted by the OS anchor.
        self.directory.sync_all()?;
        let digest = sha256::Hash::hash(&record).to_byte_array();
        anchor[64..96].copy_from_slice(&digest);
        self.credential.write(anchor)?;
        unix::renameat(&self.directory, PENDING, &self.directory, STATE)
            .map_err(|_| StorageError::Io)?;
        self.directory.sync_all()?;
        anchor[32..64].copy_from_slice(&digest);
        anchor[64..96].fill(0);
        self.credential.write(anchor)?;
        Ok(())
    }

    fn reserve(&mut self, change: bool) -> Result<String, StorageError> {
        let (mut account, mut anchor) = self.load()?;
        let address = if change {
            account.next_change_address()
        } else {
            account.next_receive_address()
        }
        .map_err(|_| StorageError::Derivation)?;
        self.commit(&account, &mut anchor)?;
        // No address escapes on uncertain I/O or OS-store failure. Reopen to recover.
        Ok(address)
    }
}

// One relative-file boundary: no symlinks, inherited descriptors or blocking FIFOs.
fn local_file(directory: &File, name: &str, flags: OFlags) -> Result<File, rustix::io::Errno> {
    unix::openat(
        directory,
        name,
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::RUSR | Mode::WUSR,
    )
    .map(File::from)
}

impl From<std::io::Error> for StorageError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

#[cfg(test)]
mod tests;
