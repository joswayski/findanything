use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

pub struct UsageStore {
    connection: Mutex<Connection>,
}

#[derive(Debug, Default)]
pub struct UsageSignals {
    pub query_count: u32,
    pub global_count: u32,
    pub last_used: i64,
}

impl UsageStore {
    pub fn open() -> Result<Self, String> {
        let database_path = database_path()?;
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create app data directory: {error}"))?;
        }

        let connection = Connection::open(database_path)
            .map_err(|error| format!("Failed to open usage database: {error}"))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 CREATE TABLE IF NOT EXISTS query_selections (
                    normalized_query TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    selection_count INTEGER NOT NULL DEFAULT 1,
                    last_selected INTEGER NOT NULL,
                    PRIMARY KEY (normalized_query, entity_id)
                 );
                 CREATE TABLE IF NOT EXISTS entity_usage (
                    entity_id TEXT PRIMARY KEY,
                    launch_count INTEGER NOT NULL DEFAULT 1,
                    last_used INTEGER NOT NULL
                 );",
            )
            .map_err(|error| format!("Failed to initialize usage database: {error}"))?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn record_selection(&self, normalized_query: &str, entity_id: &str) -> Result<(), String> {
        let now = unix_timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Usage database lock was poisoned".to_owned())?;

        connection
            .execute(
                "INSERT INTO query_selections
                    (normalized_query, entity_id, selection_count, last_selected)
                 VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT(normalized_query, entity_id) DO UPDATE SET
                    selection_count = selection_count + 1,
                    last_selected = excluded.last_selected",
                params![normalized_query, entity_id, now],
            )
            .map_err(|error| format!("Failed to save query preference: {error}"))?;
        connection
            .execute(
                "INSERT INTO entity_usage (entity_id, launch_count, last_used)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(entity_id) DO UPDATE SET
                    launch_count = launch_count + 1,
                    last_used = excluded.last_used",
                params![entity_id, now],
            )
            .map_err(|error| format!("Failed to save app usage: {error}"))?;

        Ok(())
    }

    pub fn signals_for_query(&self, normalized_query: &str) -> HashMap<String, UsageSignals> {
        let Ok(connection) = self.connection.lock() else {
            return HashMap::new();
        };

        let mut signals = HashMap::<String, UsageSignals>::new();

        if let Ok(mut statement) =
            connection.prepare("SELECT entity_id, launch_count, last_used FROM entity_usage")
            && let Ok(rows) = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
        {
            for (entity_id, global_count, last_used) in rows.flatten() {
                signals.insert(
                    entity_id,
                    UsageSignals {
                        global_count,
                        last_used,
                        ..UsageSignals::default()
                    },
                );
            }
        }

        if !normalized_query.is_empty()
            && let Ok(mut statement) = connection.prepare(
                "SELECT entity_id, selection_count FROM query_selections
                 WHERE normalized_query = ?1",
            )
            && let Ok(rows) = statement.query_map(params![normalized_query], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
        {
            for (entity_id, query_count) in rows.flatten() {
                signals.entry(entity_id).or_default().query_count = query_count;
            }
        }

        signals
    }
}

fn database_path() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|directory| directory.join("Find Anything").join("findanything.sqlite3"))
        .ok_or_else(|| "Could not determine the local application data directory".to_owned())
}

pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
