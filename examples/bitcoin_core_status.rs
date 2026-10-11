//! Explicit developer-only status check; never loads or transmits wallet material.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), monoform_wallet::core_rpc::CoreError> {
    use monoform_wallet::core_rpc::{CoreError, CoreTestnet4};
    let mut args = std::env::args_os().skip(1);
    let datadir = args.next().ok_or(CoreError::UnsafeCookie)?;
    if args.next().is_some() {
        return Err(CoreError::UnsafeCookie);
    }
    let status = CoreTestnet4::new(std::path::Path::new(&datadir))?.status()?;
    println!(
        "Testnet4: {:?}; blocks={}, headers={}, pruned={}",
        status.state, status.blocks, status.headers, status.pruned
    );
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("Core backend supports macOS and Linux only.");
    std::process::exit(1);
}
