use std::path::PathBuf;

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Application,
    SystemAction,
    File,
}

#[derive(Clone, Debug)]
pub enum LaunchTarget {
    Application(PathBuf),
    Url(String),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: String,
    pub kind: EntityKind,
    pub title: String,
    pub subtitle: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub target: LaunchTarget,
}

impl Entity {
    pub fn searchable_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.title,
            self.subtitle,
            self.aliases.join(" "),
            self.description
        )
    }

    pub fn embedding_text(&self) -> String {
        let kind = match self.kind {
            EntityKind::Application => "application",
            EntityKind::SystemAction => "computer setting and system action",
            EntityKind::File => "file",
        };

        format!(
            "Name: {}. Type: {}. Aliases and keywords: {}. Description: {}",
            self.title,
            kind,
            self.aliases.join(", "),
            self.description
        )
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub kind: EntityKind,
    pub title: String,
    pub subtitle: String,
    pub score: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub semantic_status: String,
    pub semantic_message: Option<String>,
}
