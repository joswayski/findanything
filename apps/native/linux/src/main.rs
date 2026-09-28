#[cfg(target_os = "linux")]
fn main() {
    findanything_ui::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("The Linux launcher requires Linux");
}
