use crate::model::Entity;
use std::collections::HashMap;

pub struct SemanticIndex;

impl SemanticIndex {
    pub fn warming() -> Self {
        Self
    }
    pub fn warm_in_background(&self, _entities: Vec<Entity>) {}
    pub fn status(&self) -> (String, Option<String>) {
        (
            "unavailable".into(),
            Some("This build includes keyword matching only.".into()),
        )
    }
    pub fn similarities(&self, _query: &str) -> HashMap<String, f32> {
        HashMap::new()
    }
}
