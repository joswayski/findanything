use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{Entity, LaunchTarget};

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod personal_files;
#[cfg(target_os = "windows")]
mod windows;

pub fn discover_applications() -> Vec<Entity> {
    #[cfg(target_os = "macos")]
    {
        return macos::discover_applications();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::discover_applications();
    }
    #[cfg(target_os = "linux")]
    {
        return linux::discover_applications();
    }

    #[allow(unreachable_code)]
    Vec::new()
}

pub fn search_filenames(query: &str, limit: usize) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return macos::search_filenames(query, limit);
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        return personal_files::search(query, limit);
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
    // `gio launch` parses the desktop file itself; its Exec value is never
    // passed through a shell (and neither is user input).
    spawn(Command::new("gio").arg("launch").arg(path))
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
    spawn(Command::new("gio").arg("open").arg(value))
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = spawn(Command::new("open").arg(path));

    #[cfg(target_os = "windows")]
    let result = spawn(Command::new("explorer.exe").arg(path));

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = spawn(Command::new("gio").arg("open").arg(path));

    result.map_err(|error| format!("Failed to open {}: {error}", path.display()))
}

#[cfg(target_os = "windows")]
fn spawn(command: &mut Command) -> Result<(), String> {
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch result: {error}"))
}

#[cfg(not(target_os = "windows"))]
fn spawn(command: &mut Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("Failed to launch result: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "The system launcher rejected this result ({status})."
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn rejected_os_launch_is_an_error_not_a_learned_success() {
        assert!(super::spawn(&mut std::process::Command::new("false")).is_err());
        assert!(super::spawn(&mut std::process::Command::new("true")).is_ok());
    }
}
