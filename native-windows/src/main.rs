#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(any(windows, test))]
mod ui_history;
#[cfg(windows)]
mod ui_monitor;
#[cfg(windows)]
mod ui_settings;
#[cfg(windows)]
use notmynet_windows::overlay;

#[cfg(windows)]
fn main() {
    ui_monitor::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("NotMyNet's native app requires Windows.");
}
