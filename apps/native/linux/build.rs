use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let manifest = PathBuf::from("icons.gresource.xml");
    let design = PathBuf::from("../design/lucide");
    println!("cargo:rerun-if-changed={}", manifest.display());
    for icon in ["search.svg", "x.svg"] {
        println!("cargo:rerun-if-changed={}", design.join(icon).display());
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"))
        .join("findanything-icons.gresource");
    let status = Command::new("glib-compile-resources")
        .arg(&manifest)
        .arg(format!("--sourcedir={}", design.display()))
        .arg(format!("--target={}", output.display()))
        .status()
        .expect("glib-compile-resources is required to build the GTK client");
    assert!(status.success(), "failed to compile GTK icon resources");
}
