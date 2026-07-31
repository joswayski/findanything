use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{Entity, LaunchTarget};

#[cfg(target_os = "macos")]
mod macos;

pub fn discover_applications() -> Vec<Entity> {
    #[cfg(target_os = "macos")]
    {
        return macos::discover_applications();
    }

    #[allow(unreachable_code)]
    Vec::new()
}

pub fn search_filenames(query: &str, limit: usize) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return macos::search_filenames(query, limit);
    }

    #[allow(unreachable_code)]
    Vec::new()
}

pub fn launch(target: &LaunchTarget) -> Result<(), String> {
    match target {
        LaunchTarget::Application(path) => launch_application(path),
        LaunchTarget::Url(url) => open_value(url),
        LaunchTarget::File(path) => open_path(path),
    }
}

#[cfg(target_os = "macos")]
fn launch_application(path: &Path) -> Result<(), String> {
    spawn(Command::new("open").arg("-a").arg(path))
}

#[cfg(target_os = "windows")]
fn launch_application(path: &Path) -> Result<(), String> {
    spawn(Command::new("explorer.exe").arg(path))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn launch_application(path: &Path) -> Result<(), String> {
    spawn(Command::new("xdg-open").arg(path))
}

#[cfg(target_os = "macos")]
fn open_value(value: &str) -> Result<(), String> {
    spawn(Command::new("open").arg(value))
}

#[cfg(target_os = "windows")]
fn open_value(value: &str) -> Result<(), String> {
    spawn(Command::new("explorer.exe").arg(value))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_value(value: &str) -> Result<(), String> {
    spawn(Command::new("xdg-open").arg(value))
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let command = Command::new("open").arg(path).spawn();

    #[cfg(target_os = "windows")]
    let command = Command::new("explorer.exe").arg(path).spawn();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let command = Command::new("xdg-open").arg(path).spawn();

    command
        .map(|_| ())
        .map_err(|error| format!("Failed to open {}: {error}", path.display()))
}

fn spawn(command: &mut Command) -> Result<(), String> {
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch result: {error}"))
}
