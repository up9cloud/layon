use anyhow::Result;
use layon_proto::PowerAction;

#[cfg(windows)]
pub fn apply(action: PowerAction) -> Result<()> {
    use windows_sys::Win32::System::Power::SetSuspendState;

    let hibernate = action == PowerAction::Hibernate;
    // SAFETY: plain value arguments, no pointers.
    // Wake events stay enabled, otherwise Wake-on-LAN could not bring it back.
    if unsafe { SetSuspendState(hibernate, false, false) } {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(target_os = "linux")]
pub fn apply(action: PowerAction) -> Result<()> {
    let verb = match action {
        PowerAction::Sleep => "suspend",
        PowerAction::Hibernate => "hibernate",
    };
    run("systemctl", &[verb])
}

#[cfg(target_os = "macos")]
pub fn apply(action: PowerAction) -> Result<()> {
    // macOS picks between sleep and safe sleep itself (`pmset hibernatemode`).
    let _ = action;
    run("pmset", &["sleepnow"])
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn apply(action: PowerAction) -> Result<()> {
    anyhow::bail!("{action:?} is not supported on this OS")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = std::process::Command::new(program).args(args).status()?;
    anyhow::ensure!(status.success(), "`{program} {}` exited with {status}", args.join(" "));
    Ok(())
}
