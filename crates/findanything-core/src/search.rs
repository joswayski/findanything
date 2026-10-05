use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use strsim::normalized_levenshtein;

use crate::actions;
use crate::model::{Entity, EntityKind, LaunchTarget, SearchResponse, SearchResult};
use crate::platform;
use crate::semantic::SemanticIndex;
use crate::usage::{UsageSignals, UsageStore, unix_timestamp};

const RESULT_LIMIT: usize = 10;
const FILE_RESULT_LIMIT: usize = 6;
const SEMANTIC_RESULT_LIMIT: usize = 3;
const SEMANTIC_THRESHOLD: f32 = 0.62;

// Application/action metadata is immutable for the lifetime of the engine.
// Keep its normalized search text alongside the original display/launch data.
struct IndexedEntity {
    entity: Entity,
    title: String,
    aliases: Vec<String>,
    searchable: String,
}

impl IndexedEntity {
    fn new(entity: Entity) -> Self {
        Self {
            title: normalize(&entity.title),
            aliases: entity
                .aliases
                .iter()
                .map(|alias| normalize(alias))
                .collect(),
            searchable: normalize(&entity.searchable_text()),
            entity,
        }
    }
}

pub struct SearchEngine {
    entities: Vec<IndexedEntity>,
    usage: UsageStore,
    semantic: SemanticIndex,
    transient_files: Mutex<HashMap<String, Entity>>,
}

impl SearchEngine {
    pub fn new() -> Result<Self, String> {
        let mut entities = actions::system_actions();
        entities.extend(platform::discover_applications());

        let mut seen = std::collections::HashSet::new();
        entities.retain(|entity| seen.insert(entity.id.clone()));

        // A failed database open must not leave a model worker behind on each retry.
        let usage = UsageStore::open()?;
        let semantic = SemanticIndex::warming();
        semantic.warm_in_background(entities.clone());

        Ok(Self {
            entities: entities.into_iter().map(IndexedEntity::new).collect(),
            usage,
            semantic,
            transient_files: Mutex::new(HashMap::new()),
        })
    }

    pub fn search(&self, raw_query: &str) -> SearchResponse {
        let query = normalize(raw_query);
        let semantic_scores = if query.is_empty() {
            HashMap::new()
        } else {
            self.semantic.similarities(&query)
        };
        let usage = self.usage.signals_for_query(&query);

        let mut ranked = self
            .entities
            .iter()
            .filter_map(|indexed| {
                let entity = &indexed.entity;
                rank_entity(
                    indexed,
                    &query,
                    semantic_scores.get(&entity.id).copied(),
                    usage.get(&entity.id).unwrap_or(&UsageSignals::default()),
                )
            })
            .collect::<Vec<_>>();

        if !query.is_empty() {
            let files = platform::search_filenames(raw_query.trim(), FILE_RESULT_LIMIT);
            if let Ok(mut transient_files) = self.transient_files.lock() {
                transient_files.clear();
                for path in files {
                    let entity = file_entity(&path);
                    let result = rank_file(&entity, &query);
                    transient_files.insert(entity.id.clone(), entity);
                    ranked.push(result);
                }
            }
        }

        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut semantic_results = 0;
        ranked.retain(|result| {
            if result.reason != "Semantic match" {
                return true;
            }

            semantic_results += 1;
            semantic_results <= SEMANTIC_RESULT_LIMIT
        });
        ranked.truncate(RESULT_LIMIT);

        let (semantic_status, semantic_message) = self.semantic.status();
        SearchResponse {
            results: ranked,
            semantic_status,
            semantic_message,
        }
    }

    pub fn activate(&self, id: &str, raw_query: &str) -> Result<(), String> {
        let entity = self
            .entities
            .iter()
            .map(|indexed| &indexed.entity)
            .find(|entity| entity.id == id)
            .cloned()
            .or_else(|| {
                self.transient_files
                    .lock()
                    .ok()
                    .and_then(|files| files.get(id).cloned())
            })
            .ok_or_else(|| "That result is no longer available".to_owned())?;

        platform::launch(&entity.target)?;
        self.usage
            .record_selection(&normalize(raw_query), &entity.id)
    }

    pub fn indexed_entities(&self) -> usize {
        self.entities.len()
    }
}

fn rank_entity(
    indexed: &IndexedEntity,
    query: &str,
    semantic_similarity: Option<f32>,
    usage: &UsageSignals,
) -> Option<SearchResult> {
    let entity = &indexed.entity;
    if query.is_empty() {
        let type_prior = match entity.kind {
            EntityKind::SystemAction => 18.0,
            EntityKind::Application => 15.0,
            EntityKind::File => 0.0,
        };
        let score = type_prior + usage_score(usage);
        return Some(to_result(
            entity,
            score,
            if usage.global_count > 0 {
                "Recently used"
            } else {
                "Suggested"
            },
        ));
    }

    let (lexical, lexical_reason) = lexical_score(indexed, query);
    let semantic = semantic_similarity
        .filter(|similarity| *similarity >= SEMANTIC_THRESHOLD)
        .map(|similarity| ((similarity - SEMANTIC_THRESHOLD) * 100.0).min(32.0))
        .unwrap_or_default();

    if lexical <= 0.0 && semantic <= 0.0 && usage.query_count == 0 {
        return None;
    }

    let learned = usage_score(usage);
    let score = lexical + semantic + learned;
    let reason = if usage.query_count > 0 {
        "Learned from your choices"
    } else if lexical > 0.0 {
        lexical_reason
    } else {
        "Semantic match"
    };

    Some(to_result(entity, score, reason))
}

fn lexical_score(entity: &IndexedEntity, query: &str) -> (f32, &'static str) {
    let IndexedEntity {
        title,
        aliases,
        searchable,
        ..
    } = entity;

    if title == query {
        return (110.0, "Exact name");
    }
    if aliases.iter().any(|alias| alias == query) {
        return (106.0, "Exact app keyword");
    }
    if title.starts_with(query) {
        return (92.0, "Name starts with query");
    }
    if aliases.iter().any(|alias| alias.starts_with(query)) {
        return (88.0, "Keyword starts with query");
    }

    let query_tokens = query.split_whitespace();
    if query_tokens.clone().next().is_some()
        && query_tokens.clone().all(|token| searchable.contains(token))
    {
        let title_or_alias = aliases
            .iter()
            .chain(std::iter::once(title))
            .any(|candidate| query_tokens.clone().all(|token| candidate.contains(token)));
        return if title_or_alias {
            (72.0, "Keyword match")
        } else {
            (58.0, "Metadata match")
        };
    }

    let fuzzy = aliases
        .iter()
        .chain(std::iter::once(title))
        .map(|candidate| normalized_levenshtein(query, candidate) as f32)
        .fold(0.0_f32, f32::max);
    if fuzzy >= 0.7 {
        return (fuzzy * 68.0, "Fuzzy match");
    }

    (0.0, "")
}

fn usage_score(signals: &UsageSignals) -> f32 {
    let query_preference = if signals.query_count > 0 {
        42.0 + (signals.query_count as f32).ln_1p() * 12.0
    } else {
        0.0
    };
    let global_frequency = (signals.global_count as f32).ln_1p() * 6.0;
    let recency = if signals.last_used <= 0 {
        0.0
    } else {
        let age_days = (unix_timestamp() - signals.last_used).max(0) as f32 / 86_400.0;
        10.0 * (-age_days / 21.0).exp()
    };

    query_preference + global_frequency + recency
}

fn rank_file(entity: &Entity, query: &str) -> SearchResult {
    let title = normalize(&entity.title);
    let score = if title == query {
        64.0
    } else if title.starts_with(query) {
        58.0
    } else {
        48.0
    };
    to_result(entity, score, "File name")
}

fn file_entity(path: &Path) -> Entity {
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let subtitle = path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_else(|| "File".to_owned());

    Entity {
        id: format!("file:{}", path.display()),
        kind: EntityKind::File,
        title,
        subtitle,
        aliases: Vec::new(),
        description: "A local file or folder".to_owned(),
        target: LaunchTarget::File(path.to_path_buf()),
    }
}

fn to_result(entity: &Entity, score: f32, reason: &str) -> SearchResult {
    SearchResult {
        id: entity.id.clone(),
        kind: entity.kind,
        title: entity.title.clone(),
        subtitle: entity.subtitle.clone(),
        score: (score * 10.0).round() / 10.0,
        reason: reason.to_owned(),
    }
}

pub fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{IndexedEntity, lexical_score, normalize, rank_entity};
    use crate::model::{Entity, EntityKind, LaunchTarget};
    use crate::usage::{UsageSignals, unix_timestamp};

    fn app(title: &str, aliases: &[&str]) -> Entity {
        Entity {
            id: format!("app:{title}"),
            kind: EntityKind::Application,
            title: title.to_owned(),
            subtitle: "Application".to_owned(),
            aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
            description: format!("{title} is an installed application"),
            target: LaunchTarget::Application(PathBuf::from(format!("/{title}.app"))),
        }
    }

    #[test]
    fn normalizes_punctuation_and_spacing() {
        assert_eq!(normalize("  Wi-Fi!!  Settings "), "wi fi settings");
    }

    #[test]
    fn exact_metadata_keyword_is_a_strong_match() {
        let numi = IndexedEntity::new(app("Numi", &["calculator", "unit converter"]));
        assert_eq!(lexical_score(&numi, "calculator").0, 106.0);
    }

    #[test]
    fn fuzzy_matching_accepts_typos_without_matching_merely_similar_names() {
        let calculator = IndexedEntity::new(app("Calculator", &[]));
        let calendar = IndexedEntity::new(app("Calendar", &[]));

        assert!(lexical_score(&calculator, "calcuator").0 > 0.0);
        assert_eq!(lexical_score(&calendar, "calculator").0, 0.0);
    }

    #[test]
    fn lexical_scores_preserve_unicode_metadata_and_match_precedence() {
        let mut entity = app(
            "Café Studio",
            &["Wi-Fi", "Résumé Editor", "unit", "converter"],
        );
        entity.subtitle = "Desktop tools".into();
        entity.description = "Open a project using wireless networking".into();
        let entity = IndexedEntity::new(entity);

        for (raw_query, expected) in [
            (" CAFÉ--STUDIO ", (110.0, "Exact name")),
            ("WI-FI!", (106.0, "Exact app keyword")),
            ("café", (92.0, "Name starts with query")),
            ("résumé", (88.0, "Keyword starts with query")),
            ("studio café", (72.0, "Keyword match")),
            ("editor résumé", (72.0, "Keyword match")),
            ("unit converter", (58.0, "Metadata match")),
            ("desktop wireless", (58.0, "Metadata match")),
            ("unrelated", (0.0, "")),
        ] {
            assert_eq!(
                lexical_score(&entity, &normalize(raw_query)),
                expected,
                "{raw_query}"
            );
        }

        let result = rank_entity(&entity, "café studio", None, &UsageSignals::default()).unwrap();
        assert_eq!(result.id, "app:Café Studio");
        assert_eq!(result.title, "Café Studio");
        assert_eq!(result.subtitle, "Desktop tools");
        assert_eq!(result.score, 110.0);

        let calculator = IndexedEntity::new(app("Calculator", &["calculator", "calculate"]));
        assert_eq!(
            lexical_score(&calculator, "calculator"),
            (110.0, "Exact name")
        );
        assert_eq!(
            lexical_score(&calculator, "calc"),
            (92.0, "Name starts with query")
        );
        let (score, reason) = lexical_score(&calculator, "calcuator");
        assert!((score - 61.2).abs() < 0.001);
        assert_eq!(reason, "Fuzzy match");
    }

    // Isolates catalogue matching from ONNX, SQLite, and filesystem latency.
    // Run in release mode with --ignored --nocapture to compare hot-path changes.
    #[test]
    #[ignore = "manual lexical search benchmark"]
    fn benchmark_lexical_catalogue() {
        use std::hint::black_box;
        use std::time::Instant;

        let entities = (0..500)
            .map(|index| {
                IndexedEntity::new(app(
                    &format!("Code Studio {index}"),
                    &["editor", "programming", "source code"],
                ))
            })
            .collect::<Vec<_>>();
        let queries = [
            "c",
            "co",
            "code",
            "editor",
            "source studio",
            "unmatched",
            "code stduio",
        ];
        let iterations = 100;
        let started = Instant::now();
        let mut total = 0.0;
        for _ in 0..iterations {
            for query in queries {
                for entity in &entities {
                    total += black_box(lexical_score(black_box(entity), black_box(query))).0;
                }
            }
        }
        println!(
            "500 entities, {} searches: {:?} total, {:?}/search, score checksum {}",
            iterations * queries.len(),
            started.elapsed(),
            started.elapsed() / (iterations * queries.len()) as u32,
            black_box(total),
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn brightness_metadata_resolves_to_displays() {
        let mut matches = crate::actions::system_actions()
            .into_iter()
            .map(IndexedEntity::new)
            .filter_map(|entity| rank_entity(&entity, "brightness", None, &UsageSignals::default()))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.score.total_cmp(&left.score));

        assert_eq!(
            matches.first().map(|result| result.title.as_str()),
            Some("Displays")
        );
    }

    #[test]
    fn one_explicit_choice_beats_an_exact_name_next_time() {
        let numi = IndexedEntity::new(app("Numi", &["calculator"]));
        let calculator = IndexedEntity::new(app("Calculator", &[]));
        let learned = UsageSignals {
            query_count: 1,
            global_count: 1,
            last_used: unix_timestamp(),
        };

        let numi_score = rank_entity(&numi, "calculator", None, &learned)
            .expect("Numi should match")
            .score;
        let calculator_score =
            rank_entity(&calculator, "calculator", None, &UsageSignals::default())
                .expect("Calculator should match")
                .score;

        assert!(numi_score > calculator_score);
    }
}
