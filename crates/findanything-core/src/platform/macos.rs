use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use plist::{Dictionary, Value};
use walkdir::WalkDir;

use crate::model::{Entity, EntityKind, LaunchTarget};

pub fn discover_applications() -> Vec<Entity> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];

    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }

    let mut seen = HashSet::new();
    let mut applications = Vec::new();

    for root in roots.into_iter().filter(|root| root.exists()) {
        for entry in WalkDir::new(root)
            .min_depth(1)
            .max_depth(2)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !entry.file_type().is_dir()
                || path.extension().and_then(|ext| ext.to_str()) != Some("app")
            {
                continue;
            }

            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if !seen.insert(canonical.clone()) {
                continue;
            }

            if let Some(entity) = read_application(&canonical) {
                applications.push(entity);
            }
        }
    }

    applications.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
    applications
}

fn read_application(path: &Path) -> Option<Entity> {
    let info_path = path.join("Contents/Info.plist");
    let value = Value::from_file(info_path).ok()?;
    let dictionary = value.as_dictionary()?;

    let fallback_name = path.file_stem()?.to_string_lossy().into_owned();
    let name = string(dictionary, "CFBundleDisplayName")
        .or_else(|| string(dictionary, "CFBundleName"))
        .unwrap_or(fallback_name);
    let bundle_id = string(dictionary, "CFBundleIdentifier")
        .unwrap_or_else(|| format!("path:{}", path.display()));

    let mut aliases = vec![name.clone()];
    aliases.extend(keywords(dictionary));
    aliases.extend(document_type_names(dictionary));

    if let Some(category) = string(dictionary, "LSApplicationCategoryType") {
        aliases.push(humanize_category(&category));
    }

    aliases.sort_by_key(|value| value.to_lowercase());
    aliases.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    let description = if aliases.len() > 1 {
        format!(
            "{} is an installed macOS application associated with {}.",
            name,
            aliases
                .iter()
                .skip(1)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("{} is an installed macOS application.", name)
    };

    Some(Entity {
        id: format!("app:{bundle_id}"),
        kind: EntityKind::Application,
        title: name,
        subtitle: "Application".to_owned(),
        aliases,
        description,
        target: LaunchTarget::Application(path.to_path_buf()),
    })
}

fn string(dictionary: &Dictionary, key: &str) -> Option<String> {
    dictionary
        .get(key)
        .and_then(Value::as_string)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn keywords(dictionary: &Dictionary) -> Vec<String> {
    let Some(value) = dictionary.get("MDItemKeywords") else {
        return Vec::new();
    };

    match value {
        Value::String(value) => value
            .split([',', ';'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_string)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn document_type_names(dictionary: &Dictionary) -> Vec<String> {
    dictionary
        .get("CFBundleDocumentTypes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_dictionary)
        .filter_map(|document_type| string(document_type, "CFBundleTypeName"))
        .collect()
}

fn humanize_category(category: &str) -> String {
    category
        .rsplit('.')
        .next()
        .unwrap_or(category)
        .replace('-', " ")
}

pub fn search_filenames(query: &str, limit: usize) -> Vec<PathBuf> {
    if query.trim().chars().count() < 3 {
        return Vec::new();
    }

    let Ok(output) = Command::new("mdfind").arg("-name").arg(query).output() else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .filter(|path| is_useful_file(path))
        .take(limit)
        .collect()
}

fn is_useful_file(path: &Path) -> bool {
    let path_text = path.to_string_lossy();
    let in_personal_files = dirs::home_dir()
        .map(|home| path.starts_with(&home) && !path.starts_with(home.join("Library")))
        .unwrap_or(false);
    let on_external_volume = path.starts_with("/Volumes");
    let contains_hidden_component = path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|component| component.starts_with('.') && component.len() > 1)
    });

    (in_personal_files || on_external_volume)
        && !contains_hidden_component
        && !path_text.contains("/Library/Caches/")
        && !path_text.contains("/node_modules/")
        && !path_text.contains("/target/")
        && path.extension().and_then(|extension| extension.to_str()) != Some("app")
}

#[cfg(test)]
mod tests {
    use super::{discover_applications, is_useful_file};

    #[test]
    fn filename_results_stay_out_of_library_and_build_folders() {
        let home = dirs::home_dir().expect("macOS should have a home directory");

        assert!(is_useful_file(&home.join("Documents/notes/calculator.txt")));
        assert!(!is_useful_file(
            &home.join("Library/Application Support/Browser/calculator.svg")
        ));
        assert!(!is_useful_file(
            &home.join("Documents/project/node_modules/calculator.js")
        ));
        assert!(!is_useful_file(std::path::Path::new(
            "/Library/Developer/SDKs/calculator.tbd"
        )));
    }

    #[test]
    fn installed_numi_exposes_its_calculator_keyword_when_present() {
        let applications = discover_applications();
        let Some(numi) = applications
            .iter()
            .find(|application| application.title.eq_ignore_ascii_case("Numi"))
        else {
            return;
        };

        assert!(
            numi.aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case("calculator"))
        );
    }
}
