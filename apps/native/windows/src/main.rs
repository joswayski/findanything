#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    findanything_ui::run();
}

#[cfg(not(windows))]
fn main() {
    panic!("The Windows launcher requires Windows");
}
