//! A per-user election lock and a payload-free activation mailbox.
//! The lock is never deleted: removing it would allow two inode owners.
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

pub struct Instance {
    _lock: File,
    activation: PathBuf,
}

impl Instance {
    pub fn acquire() -> Result<Option<Self>, String> {
        let root = dirs::data_local_dir()
            .ok_or("Local application data directory is unavailable")?
            .join("Find Anything");
        Self::acquire_at(&root).map_err(|e| format!("Cannot acquire launcher instance: {e}"))
    }

    fn acquire_at(root: &Path) -> std::io::Result<Option<Self>> {
        fs::create_dir_all(root)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(root.join("native.lock"))?;
        let activation = root.join("native.activate");
        match lock.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self {
                _lock: lock,
                activation,
            })),
            Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                // create_new never follows a pre-existing symlink; coalescing
                // repeated focus requests is intentional.
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(activation)
                {
                    Ok(_) => Ok(None),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    pub fn take_activation_request(&self) -> bool {
        fs::remove_file(&self.activation).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_coalesces_and_releases_without_deleting_lock() {
        let dir = tempfile::tempdir().unwrap();
        let first = Instance::acquire_at(dir.path()).unwrap().unwrap();
        assert!(!first.take_activation_request());
        assert!(Instance::acquire_at(dir.path()).unwrap().is_none());
        assert!(Instance::acquire_at(dir.path()).unwrap().is_none());
        assert!(first.take_activation_request());
        assert!(!first.take_activation_request());
        drop(first);
        assert!(Instance::acquire_at(dir.path()).unwrap().is_some());
        assert!(dir.path().join("native.lock").exists());
    }
}
