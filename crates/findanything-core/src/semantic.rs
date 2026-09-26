use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

use crate::model::Entity;

#[derive(Clone)]
pub struct SemanticIndex {
    state: Arc<Mutex<SemanticState>>,
}

enum SemanticState {
    Warming,
    Ready(Box<SemanticRuntime>),
    Unavailable(String),
}

struct SemanticRuntime {
    model: TextEmbedding,
    vectors: HashMap<String, Vec<f32>>,
}

impl SemanticIndex {
    pub fn warming() -> Self {
        Self {
            state: Arc::new(Mutex::new(SemanticState::Warming)),
        }
    }

    pub fn warm_in_background(&self, entities: Vec<Entity>) {
        let state = Arc::clone(&self.state);
        std::thread::Builder::new()
            .name("semantic-index".to_owned())
            .spawn(move || match build_runtime(&entities) {
                Ok(runtime) => {
                    println!("Local semantic index is ready");
                    if let Ok(mut state) = state.lock() {
                        *state = SemanticState::Ready(Box::new(runtime));
                    }
                }
                Err(error) => {
                    eprintln!("{error}");
                    if let Ok(mut state) = state.lock() {
                        *state = SemanticState::Unavailable(error);
                    }
                }
            })
            .expect("failed to spawn semantic index thread");
    }

    pub fn status(&self) -> (String, Option<String>) {
        let Ok(state) = self.state.lock() else {
            return (
                "unavailable".to_owned(),
                Some("Semantic index lock was poisoned".to_owned()),
            );
        };

        match &*state {
            SemanticState::Warming => ("warming".to_owned(), None),
            SemanticState::Ready(_) => ("ready".to_owned(), None),
            SemanticState::Unavailable(message) => {
                ("unavailable".to_owned(), Some(message.clone()))
            }
        }
    }

    pub fn similarities(&self, query: &str) -> HashMap<String, f32> {
        let Ok(mut state) = self.state.lock() else {
            return HashMap::new();
        };
        let SemanticState::Ready(runtime) = &mut *state else {
            return HashMap::new();
        };

        let Ok(mut embeddings) = runtime.model.embed(vec![query], None) else {
            return HashMap::new();
        };
        let Some(query_vector) = embeddings.pop() else {
            return HashMap::new();
        };

        runtime
            .vectors
            .iter()
            .map(|(id, vector)| (id.clone(), cosine_similarity(&query_vector, vector)))
            .collect()
    }
}

fn build_runtime(entities: &[Entity]) -> Result<SemanticRuntime, String> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| "Local model cache directory is unavailable".to_owned())?
        .join("Find Anything")
        .join("models");
    fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("Local model cache could not be created: {error}"))?;

    let mut model = TextEmbedding::try_new(
        TextInitOptions::new(EmbeddingModel::BGESmallENV15Q)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(false),
    )
    .map_err(|error| format!("Local embedding model could not be loaded: {error}"))?;

    let texts = entities
        .iter()
        .map(Entity::embedding_text)
        .collect::<Vec<_>>();
    let embeddings = model
        .embed(texts, None)
        .map_err(|error| format!("Application metadata could not be embedded: {error}"))?;

    let vectors = entities
        .iter()
        .zip(embeddings)
        .map(|(entity, vector)| (entity.id.clone(), vector))
        .collect();

    Ok(SemanticRuntime { model, vectors })
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let (mut dot, mut left_norm, mut right_norm) = (0.0, 0.0, 0.0);
    for (left, right) in left.iter().zip(right) {
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }

    dot / (left_norm.sqrt() * right_norm.sqrt())
}

#[cfg(test)]
mod tests {
    use super::cosine_similarity;

    #[test]
    fn cosine_similarity_handles_identical_and_opposite_vectors() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < f32::EPSILON);
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < f32::EPSILON);
    }
}
