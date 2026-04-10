// Detect whether sz/rz tools are available on the system
use std::process::Command;
use stealthterm_config::i18n::t;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZmodemSupport {
    Available,        // sz/rz both available
    NotInstalled,     // not installed
    WindowsNoSupport, // Windows not supported
}

pub fn detect_zmodem_support() -> ZmodemSupport {
    // Windows check
    if cfg!(target_os = "windows") {
        // Check for sz.exe and rz.exe on Windows
        if check_command("sz.exe") && check_command("rz.exe") {
            return ZmodemSupport::Available;
        }
        return ZmodemSupport::WindowsNoSupport;
    }

    // Unix check
    if check_command("sz") && check_command("rz") {
        return ZmodemSupport::Available;
    }

    ZmodemSupport::NotInstalled
}

fn check_command(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .is_ok()
}

pub fn get_install_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        t("zmodem.windows_hint")
    } else if cfg!(target_os = "macos") {
        "macOS: brew install lrzsz"
    } else {
        t("zmodem.linux_hint")
    }
}
