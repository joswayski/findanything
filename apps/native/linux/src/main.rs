#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    linux::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("The GTK launcher requires Linux");
}
