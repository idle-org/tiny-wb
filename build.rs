//! Application-only resources; library-only builds need neither Git nor windres.

#[cfg(feature = "tui")]
fn main() -> std::io::Result<()> {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()?;
    }
    // Source archives and path dependencies may not have Git installed/a checkout.
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=GIT_HASH={}", revision.trim());
    Ok(())
}

#[cfg(not(feature = "tui"))]
fn main() {}
