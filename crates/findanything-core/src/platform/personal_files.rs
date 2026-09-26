use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

pub fn search(query: &str, limit: usize) -> Vec<PathBuf> {
    let needle = query.trim().to_lowercase();
    if needle.chars().count() < 3 || limit == 0 {
        return Vec::new();
    }
    static INDEX: OnceLock<Mutex<(Instant, Vec<PathBuf>)>> = OnceLock::new();
    let mut index = INDEX
        .get_or_init(|| Mutex::new((Instant::now(), scan(&roots()))))
        .lock()
        .unwrap();
    if index.0.elapsed() > Duration::from_secs(60) {
        *index = (Instant::now(), scan(&roots()));
    }
    index
        .1
        .iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().to_lowercase().contains(&needle))
        })
        .take(limit)
        .cloned()
        .collect()
}

fn roots() -> Vec<PathBuf> {
    // Resolve XDG user-dirs / Windows Known Folders, including localized and
    // redirected Documents/Desktop directories (not hardcoded English paths).
    [
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::picture_dir(),
        dirs::audio_dir(),
        dirs::video_dir(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn ignored(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    name.starts_with('.')
        || matches!(
            name.as_str(),
            "library" | "appdata" | "node_modules" | "target"
        )
}

fn scan(roots: &[PathBuf]) -> Vec<PathBuf> {
    let started = Instant::now();
    roots
        .iter()
        .flat_map(|root| {
            WalkDir::new(root)
                .follow_links(false)
                .max_depth(6)
                .into_iter()
                .filter_entry(|entry| entry.depth() == 0 || !ignored(entry.path()))
        })
        .take(20_000)
        .take_while(|_| started.elapsed() < Duration::from_millis(300))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_includes_unicode_but_excludes_hidden_build_and_symlink_entries() {
        let tmp = tempfile::tempdir().unwrap();
        for directory in [".hidden", "node_modules", "target", "notes"] {
            std::fs::create_dir(tmp.path().join(directory)).unwrap();
            std::fs::write(tmp.path().join(directory).join("résumé.txt"), "").unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("notes"), tmp.path().join("alias")).unwrap();
        let files = scan(&[tmp.path().to_owned()]);
        assert_eq!(files, vec![tmp.path().join("notes/résumé.txt")]);
    }
}
