use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::model::{Entity, EntityKind, LaunchTarget};

pub fn discover_applications() -> Vec<Entity> {
    discover_in_roots(&application_roots(), &current_desktops()).0
}

pub(crate) fn discover_settings() -> Vec<Entity> {
    discover_in_roots(&application_roots(), &current_desktops()).1
}

fn application_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| dirs::home_dir().map(|p| p.join(".local/share")))
    {
        roots.push(data.join("applications"));
    }
    let system =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(
        std::env::split_paths(&system)
            .filter(|p| p.is_absolute())
            .map(|p| p.join("applications")),
    );
    roots
}

fn current_desktops() -> HashSet<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn discover_in_roots(roots: &[PathBuf], desktops: &HashSet<String>) -> (Vec<Entity>, Vec<Entity>) {
    let mut claimed = HashSet::new();
    let mut apps = Vec::new();
    let mut settings = Vec::new();
    // Earlier XDG roots have precedence. Claim the desktop ID before parsing so
    // a user Hidden=true entry masks the system entry with the same ID.
    for root in roots {
        for entry in WalkDir::new(root)
            .follow_links(false)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.path().is_file()
                || entry.path().extension().and_then(|e| e.to_str()) != Some("desktop")
            {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            let id = relative.to_string_lossy().replace(['/', '\\'], "-");
            if !claimed.insert(id.clone()) {
                continue;
            }
            let Some(fields) = parse_desktop(entry.path()) else {
                continue;
            };
            if !visible(&fields, desktops) {
                continue;
            }
            let name = fields.get("Name").cloned().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let aliases = metadata(&fields);
            let comment = fields
                .get("Comment")
                .cloned()
                .unwrap_or_else(|| format!("{name} is an installed Linux application."));
            let entity = Entity {
                id: format!("app:{id}"),
                kind: EntityKind::Application,
                title: name,
                subtitle: "Application".into(),
                aliases,
                description: comment,
                target: LaunchTarget::Application(entry.path().to_owned()),
            };
            let is_settings = fields.get("Categories").is_some_and(|v| {
                v.split(';')
                    .any(|c| matches!(c, "Settings" | "X-GNOME-Settings-Panel"))
            }) || id.contains("control-center");
            if is_settings {
                let mut action = entity.clone();
                action.id = format!("action:linux:{id}");
                action.kind = EntityKind::SystemAction;
                action.subtitle = "System Settings".into();
                settings.push(action);
            } else {
                apps.push(entity);
            }
        }
    }
    apps.sort_by_key(|e| e.title.to_lowercase());
    settings.sort_by_key(|e| e.title.to_lowercase());
    (apps, settings)
}

fn parse_desktop(path: &Path) -> Option<HashMap<String, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut result = HashMap::new();
    let mut in_entry = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            // Keep the unlocalized value: locale resolution varies by process,
            // while this deterministic fallback is always searchable.
            if !key.contains('[') {
                result.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    Some(result)
}

fn visible(f: &HashMap<String, String>, desktops: &HashSet<String>) -> bool {
    if f.get("Type").is_none_or(|v| v != "Application")
        || truthy(f.get("Hidden"))
        || truthy(f.get("NoDisplay"))
    {
        return false;
    }
    if let Some(only) = f.get("OnlyShowIn")
        && !only
            .split(';')
            .any(|d| desktops.contains(&d.to_ascii_lowercase()))
    {
        return false;
    }
    if let Some(not) = f.get("NotShowIn")
        && not
            .split(';')
            .any(|d| desktops.contains(&d.to_ascii_lowercase()))
    {
        return false;
    }
    f.get("TryExec")
        .is_none_or(|program| executable_exists(program))
}

fn truthy(value: Option<&String>) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn executable_exists(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &Path| {
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let path = Path::new(program);
    if path.is_absolute() {
        return executable(path);
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|p| executable(&p.join(program))))
}

fn metadata(f: &HashMap<String, String>) -> Vec<String> {
    let mut values = Vec::new();
    for key in ["Name", "GenericName", "Keywords", "Comment"] {
        if let Some(value) = f.get(key) {
            values.extend(
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    values.sort_by_key(|v| v.to_lowercase());
    values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_visibility_metadata_and_hidden_override() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("user");
        let system = tmp.path().join("system");
        std::fs::create_dir_all(user.join("nested")).unwrap();
        std::fs::create_dir_all(system.join("nested")).unwrap();
        std::fs::write(
            user.join("nested/tool.desktop"),
            "[Desktop Entry]\nHidden=true\n",
        )
        .unwrap();
        std::fs::write(
            system.join("nested/tool.desktop"),
            "[Desktop Entry]\nType=Application\nName=Masked\n",
        )
        .unwrap();
        std::fs::write(system.join("calc.desktop"), "[Desktop Entry]\nType=Application\nName=Calculator\nGenericName=Math Tool\nKeywords=math;sum;\nComment=Does sums\nOnlyShowIn=GNOME;\n").unwrap();
        let (apps, _) = discover_in_roots(&[user, system], &HashSet::from(["gnome".into()]));
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].title, "Calculator");
        assert!(apps[0].aliases.contains(&"sum".into()));
    }
}
