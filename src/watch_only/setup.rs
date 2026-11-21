//! File input is bounded and validated before any destination or credential access.
use super::{Completion, Failure, Outcome, valid_path};
use monoform_wallet::{BitcoinTestnetAccount, storage::PublicStateStore};
use rustix::fs::{self as unix, Mode, OFlags};
use std::{fs::File, io::Read, os::unix::fs::MetadataExt, path::Path};
use zeroize::Zeroizing;

pub(super) fn read_export(path: &Path) -> Result<String, Failure> {
    if !path.to_str().is_some_and(valid_path) {
        return Err(Failure::ExportFile);
    }
    // Open first, then inspect the held file: no metadata/open substitution race,
    // final symlinks, inherited descriptor or blocking FIFO. Parents remain trusted.
    let file = File::from(
        unix::openat(
            unix::CWD,
            path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Failure::ExportFile)?,
    );
    let metadata = file.metadata().map_err(|_| Failure::ExportFile)?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err(Failure::ExportFile);
    }
    // Wipe our bounded raw buffer even if someone selected a secret file by mistake.
    // Only a regenerated, validated public record may reach the UI.
    let mut bytes = Zeroizing::new(Vec::with_capacity(257));
    file.take(257)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::ExportFile)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Failure::InvalidExport)?;
    BitcoinTestnetAccount::from_public_state(text)
        .map(|account| account.encode_public_state())
        .map_err(|_| Failure::InvalidExport)
}

pub(super) fn create(path: &Path, reviewed: &str) -> Outcome {
    // Revalidate the exact approved snapshot, not a source file that could have changed.
    let account =
        BitcoinTestnetAccount::from_public_state(reviewed).map_err(|_| Failure::InvalidExport)?;
    PublicStateStore::create(path, &account).map_err(Failure::Create)?;
    Ok(Completion::Created)
}
