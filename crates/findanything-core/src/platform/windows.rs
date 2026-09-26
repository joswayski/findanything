use std::collections::HashSet;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::model::{Entity, EntityKind, LaunchTarget};

pub fn discover_applications() -> Vec<Entity> {
    let mut roots = Vec::new();
    if let Some(data) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(data).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Some(data) = std::env::var_os("PROGRAMDATA") {
        roots.push(PathBuf::from(data).join("Microsoft/Windows/Start Menu/Programs"));
    }
    discover_in_roots(&roots)
}

fn discover_in_roots(roots: &[PathBuf]) -> Vec<Entity> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .follow_links(false)
            .max_depth(12)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !entry.file_type().is_file()
                || !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
            {
                continue;
            }
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .trim()
                .to_owned();
            if name.is_empty() || !seen.insert(name.to_lowercase()) {
                continue;
            }
            result.push(Entity {
                id: format!(
                    "app:windows-start-menu:{}",
                    path.to_string_lossy().to_lowercase()
                ),
                kind: EntityKind::Application,
                title: name.clone(),
                subtitle: "Application".into(),
                aliases: vec![name.clone()],
                description: format!("{name} is an application in the Windows Start Menu."),
                target: LaunchTarget::Application(path.to_owned()),
            });
        }
    }
    result.sort_by_key(|e| e.title.to_lowercase());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_nested_shortcuts() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("Tools")).unwrap();
        std::fs::write(tmp.path().join("Tools/Café App.lnk"), b"").unwrap();
        let apps = discover_in_roots(&[tmp.path().to_owned()]);
        assert_eq!(apps[0].title, "Café App");
    }
}
