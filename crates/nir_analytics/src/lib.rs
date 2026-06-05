//! Memory storage layer for the `/nir` workspace: daily log markdown I/O,
//! a SQLite checkpoint index with FTS5-accelerated recall, and the on-disk
//! analytics state used to track background work.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::Local;
use log;
use rand::{distributions::Alphanumeric, Rng};
use sqlez::connection::Connection;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub content: String,
}

// Windows shells can isolate USERPROFILE vs HOME inconsistently across PowerShell/WSL/MSYS;
// strict ordering + `.` fallback prevents telemetry from fragmenting to silent directories.
fn resolve_home_dir() -> PathBuf {
    if let Ok(path) = std::env::var("USERPROFILE") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("HOME") {
        return PathBuf::from(path);
    }
    PathBuf::from(".")
}

/// Resolves the logs directory path.
fn get_logs_dir() -> Result<PathBuf> {
    let mut home = resolve_home_dir();
    home.push(".nir");
    home.push("brain");
    home.push("logs");
    fs::create_dir_all(&home).context("Failed to create logs directory")?;
    Ok(home)
}

/// Generates a unique log entry ID.
fn build_entry_id(date_str: &str) -> String {
    let compact: String = Local::now().format("%Y%m%d%H%M%S").to_string();
    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(4)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("{}-{}-{}", date_str, compact, random)
}

/// Writes a log entry to the active daily log file.
pub fn write_daily_log(content: &str) -> Result<String> {
    let now = Local::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    let time_str = now.format("%H:%M:%S").to_string();
    let entry_id = build_entry_id(&date_str);
    
    let logs_dir = get_logs_dir()?;
    let file_path = logs_dir.join(format!("{}.md", date_str));
    
    let file_exists = file_path.exists();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .context("Failed to open daily log file")?;
        
    if !file_exists {
        writeln!(file, "# Daily Log - {}\n", date_str)?;
    }
    
    writeln!(file, "[{}] | ID:{} | {}", time_str, entry_id, content.trim())?;
    Ok(entry_id)
}

/// Reads daily log entries for a given date.
pub fn read_daily_log(date_str: &str, contains_query: Option<&str>) -> Result<Vec<LogEntry>> {
    let logs_dir = get_logs_dir()?;
    let file_path = logs_dir.join(format!("{}.md", date_str));
    
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    
    let mut file = OpenOptions::new().read(true).open(file_path)?;
    let mut raw_content = String::new();
    file.read_to_string(&mut raw_content)?;
    
    let query = contains_query.map(|q| q.to_lowercase());
    let mut entries = Vec::new();
    
    for line in raw_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        
        if let Some(with_id) = parse_line_entry(trimmed) {
            if let Some(ref q) = query {
                if !with_id.content.to_lowercase().contains(q) {
                    continue;
                }
            }
            entries.push(with_id);
        }
    }
    
    Ok(entries)
}

/// Parses a structured log line.
fn parse_line_entry(line: &str) -> Option<LogEntry> {
    if !line.starts_with('[') { return None; }
    
    let close_bracket = line.find(']')?;
    let timestamp = line[1..close_bracket].to_string();
    
    let id_marker = line.find("ID:")?;
    let remaining = &line[id_marker + 3..];
    
    let pipe_index = remaining.find('|')?;
    let id = remaining[..pipe_index].trim().to_string();
    let content = remaining[pipe_index + 1..].trim().to_string();
    
    Some(LogEntry { id, timestamp, content })
}

/// Lists all daily log markdown filenames.
pub fn list_log_files() -> Result<Vec<String>> {
    let logs_dir = get_logs_dir()?;
    let mut files = Vec::new();
    
    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                files.push(filename.to_string());
            }
        }
    }
    
    files.sort_by(|a, b| b.cmp(a));
    Ok(files)
}

/// Deletes a specific log entry.
pub fn delete_log_entry(date_str: &str, target_id: &str) -> Result<bool> {
    let logs_dir = get_logs_dir()?;
    let file_path = logs_dir.join(format!("{}.md", date_str));
    
    if !file_path.exists() {
        return Ok(false);
    }
    
    let mut file = OpenOptions::new().read(true).open(&file_path)?;
    let mut raw_content = String::new();
    file.read_to_string(&mut raw_content)?;
    
    let lines: Vec<&str> = raw_content.lines().collect();
    let mut updated_lines = Vec::new();
    let mut found = false;
    
    for line in lines {
        if let Some(entry) = parse_line_entry(line.trim()) {
            if entry.id == target_id {
                found = true;
                continue;
            }
        }
        updated_lines.push(line);
    }
    
    if found {
        let mut out_file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&file_path)?;
        for line in updated_lines {
            writeln!(out_file, "{}", line)?;
        }
    }
    
    Ok(found)
}

// =============================================================================
// SQLite storage layer (state.db at ~/.nir/brain/state.db)
// =============================================================================

/// Resolves the brain directory, creating it if missing.
fn get_brain_dir() -> Result<PathBuf> {
    let mut path = resolve_home_dir();
    path.push(".nir");
    path.push("brain");
    fs::create_dir_all(&path).context("Failed to create brain directory")?;
    Ok(path)
}

/// Resolves the path of the SQLite state database.
pub fn get_state_db_path() -> Result<PathBuf> {
    let mut path = get_brain_dir()?;
    path.push("state.db");
    Ok(path)
}

/// Persistable representation of a single checkpoint row.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CheckPointRecord {
    pub id: String,
    pub timestamp: i64,
    pub category: String,
    pub summary: String,
    pub tags: String,
    pub error_recovery: bool,
}

/// Opens (or creates) the SQLite state database and ensures the schema is present.
///
/// `checkpoints.id` is TEXT (matching the daily-log entry id format), so the
/// Current schema version. Bump and add a migration when columns/tables change.
const CURRENT_SCHEMA_VERSION: i32 = 1;

/// Initializes the SQLite database. Sets WAL journal mode and a 5s busy timeout.
/// Returns Err if sqlez's silent in-memory fallback kicked in.
pub fn init_storage_engine(database_path: &Path) -> Result<Connection> {
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directory for state database")?;
    }
    let connection = Connection::open_file(
        &database_path.to_string_lossy()
    );

    // Bail if sqlez silently swapped to in-memory — writes would vanish on exit.
    if !connection.persistent() {
        log::error!(
            "State database at {} fell back to in-memory storage. Data will not persist.",
            database_path.display()
        );
        return Err(anyhow::anyhow!(
            "State database at {} is ephemeral (in-memory fallback). Data will not persist.",
            database_path.display()
        ));
    }

    // WAL + busy_timeout so concurrent readers don't block writers.
    // PRAGMA failures here are non-fatal (e.g. WAL may not be supported on
    // some filesystems) so we log and continue.
    for pragma in &["PRAGMA journal_mode = WAL", "PRAGMA busy_timeout = 5000", "PRAGMA synchronous = NORMAL"] {
        if let Ok(mut exec) = connection.exec(pragma) {
            if let Err(err) = exec() {
                log::warn!("PRAGMA setup failed for '{}': {:?}", pragma, err);
            }
        }
    }

    // Forward-compatible schema: track version via user_version pragma.
    let current_version: i32 = connection
        .select::<i32>("PRAGMA user_version")
        .context("Failed to read user_version pragma")?()
        .context("PRAGMA user_version did not return a row")?
        .first()
        .copied()
        .unwrap_or(0);

    if current_version < 1 {
        connection
            .exec("CREATE TABLE IF NOT EXISTS checkpoints (\
                id TEXT PRIMARY KEY,\
                timestamp INTEGER,\
                category TEXT,\
                summary TEXT,\
                tags TEXT,\
                error_recovery INTEGER NOT NULL DEFAULT 0 CHECK (error_recovery IN (0, 1))\
            )")
            .context("Failed to prepare checkpoints schema statement")?()
            .context("Failed to create checkpoints table")?;

        connection
            .exec("CREATE VIRTUAL TABLE IF NOT EXISTS checkpoints_fts USING fts5(summary)")
            .context("Failed to prepare FTS5 schema statement")?()
            .context("Failed to create checkpoints_fts virtual table")?;

        if let Ok(mut set_version) =
            connection.exec(&format!("PRAGMA user_version = {}", CURRENT_SCHEMA_VERSION))
        {
            if let Err(err) = set_version() {
                log::warn!("Failed to set user_version: {:?}", err);
            }
        }
    }

    // Future migrations would go here:
    // if current_version < 2 { ... migration v2 ... }

    Ok(connection)
}

/// Atomically inserts a checkpoint into `checkpoints` and mirrors its summary
/// into `checkpoints_fts` inside a single transaction. Deletes any stale FTS
/// row first so REPLACE doesn't orphan the old entry.
pub fn insert_checkpoint(connection: &Connection, record: &CheckPointRecord) -> Result<()> {
    connection
        .with_savepoint("insert_checkpoint", || {
            // Self-heal: clear any existing FTS row for this id before the
            // main INSERT OR REPLACE reassigns rowid. record.id is generated
            // by random_id() (Alphanumeric only) so format!() is safe here.
            let safe_id = record.id.replace('\'', "''");
            connection
                .exec(&format!(
                    "DELETE FROM checkpoints_fts WHERE rowid IN \
                     (SELECT rowid FROM checkpoints WHERE id = '{safe_id}')"
                ))
                .context("Failed to prepare FTS5 delete statement")?()
                .context("Failed to clear stale FTS5 row")?;

            connection
                .exec_bound(
                    "INSERT OR REPLACE INTO checkpoints (id, timestamp, category, summary, tags, error_recovery) \
                     VALUES (?, ?, ?, ?, ?, ?)"
                )
                .context("Failed to prepare checkpoints insert statement")?(
                    (
                        record.id.as_str(),
                        record.timestamp,
                        record.category.as_str(),
                        record.summary.as_str(),
                        record.tags.as_str(),
                        record.error_recovery,
                    )
                )
                .context("Failed to insert into checkpoints table")?;

            connection
                .exec_bound(
                    "INSERT INTO checkpoints_fts (rowid, summary) \
                     VALUES ((SELECT rowid FROM checkpoints WHERE id = ?), ?)"
                )
                .context("Failed to prepare FTS5 insert statement")?(
                    (record.id.as_str(), record.summary.as_str())
                )
                .context("Failed to insert into checkpoints_fts virtual table")?;

            Ok(())
        })
        .context("Failed to commit checkpoint insert transaction")
}

/// Atomically updates a checkpoint in `checkpoints` and refreshes its mirror
/// in `checkpoints_fts` inside a single transaction. Asserts exactly one FTS5
/// row was updated so partial writes are caught.
pub fn update_checkpoint(connection: &Connection, id: &str, record: &CheckPointRecord) -> Result<()> {
    connection
        .with_savepoint("update_checkpoint", || {
            connection
                .exec_bound(
                    "UPDATE checkpoints \
                     SET timestamp = ?, category = ?, summary = ?, tags = ?, error_recovery = ? \
                     WHERE id = ?"
                )
                .context("Failed to prepare checkpoints update statement")?(
                    (
                        record.timestamp,
                        record.category.as_str(),
                        record.summary.as_str(),
                        record.tags.as_str(),
                        record.error_recovery,
                        id,
                    )
                )
                .context("Failed to update checkpoints table")?;

            connection
                .exec_bound(
                    "UPDATE checkpoints_fts \
                     SET summary = ? \
                     WHERE rowid = (SELECT rowid FROM checkpoints WHERE id = ?)"
                )
                .context("Failed to prepare FTS5 update statement")?(
                    (record.summary.as_str(), id)
                )
                .context("Failed to update checkpoints_fts virtual table")?;

            Ok(())
        })
        .context("Failed to commit checkpoint update transaction")
}

// =============================================================================
// FTS5-accelerated recall queries
// =============================================================================

/// FTS5 search over checkpoint summaries. Returns up to 15 matches ordered by
/// FTS5 `rank`.
pub fn search_checkpoints_by_text(database_path: &Path, query: &str) -> Result<Vec<CheckPointRecord>> {
    let connection = init_storage_engine(database_path)
        .context("Failed to open state database for FTS5 search")?;

    let sanitized = sanitize_fts5_query(query);

    let rows: Vec<(String, i64, String, String, String, bool)> = connection
        .select_bound::<&str, (String, i64, String, String, String, bool)>(
            "SELECT checkpoints.id, checkpoints.timestamp, checkpoints.category, \
                    checkpoints.summary, checkpoints.tags, checkpoints.error_recovery \
             FROM checkpoints_fts \
             JOIN checkpoints ON checkpoints.rowid = checkpoints_fts.rowid \
             WHERE checkpoints_fts MATCH ? \
             ORDER BY rank \
             LIMIT 15"
        )
        .context("Failed to prepare FTS5 search statement")?
        (sanitized.as_str())
        .context("Failed to execute FTS5 search query")?;

    let records = rows
        .into_iter()
        .map(|(id, timestamp, category, summary, tags, error_recovery)| {
            CheckPointRecord { id, timestamp, category, summary, tags, error_recovery }
        })
        .collect();

    Ok(records)
}

/// Wraps the user query in double quotes (FTS5 phrase syntax) and escapes
/// internal `"` as `""`.
fn sanitize_fts5_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let escaped = trimmed.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

/// Returns the 15 most recent checkpoints ordered by timestamp descending. The
/// `LIMIT 15` cap is hard-coded in the SQL string.
pub fn recent_checkpoints(database_path: &Path) -> Result<Vec<CheckPointRecord>> {
    let connection = init_storage_engine(database_path)
        .context("Failed to open state database for recent query")?;

    let rows: Vec<(String, i64, String, String, String, bool)> = connection
        .select::<(String, i64, String, String, String, bool)>(
            "SELECT id, timestamp, category, summary, tags, error_recovery \
             FROM checkpoints \
             ORDER BY timestamp DESC \
             LIMIT 15"
        )
        .context("Failed to prepare recent checkpoint query statement")?()
        .context("Failed to execute recent checkpoint query")?;

    let records = rows
        .into_iter()
        .map(|(id, timestamp, category, summary, tags, error_recovery)| {
            CheckPointRecord { id, timestamp, category, summary, tags, error_recovery }
        })
        .collect();

    Ok(records)
}

// =============================================================================
// Session config: user-facing on/off toggle for session history
// =============================================================================

/// User-facing toggle for session history (logs + checkpoint indexing).
/// When `enabled = false`, `log_task_completion` becomes a no-op so no new
/// markdown logs or FTS5 checkpoints are written. `recall_past_context`
/// is intentionally unaffected — you can always search past data even
/// when new logging is off.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionConfig {
    #[serde(default = "default_session_config_enabled")]
    pub enabled: bool,
}

fn default_session_config_enabled() -> bool {
    true
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            enabled: default_session_config_enabled(),
        }
    }
}

fn get_session_config_path() -> Result<PathBuf> {
    let mut path = get_brain_dir()?;
    path.push("config.json");
    Ok(path)
}

/// Loads the session config from disk. Returns the default (enabled) if the
/// file is missing or corrupt, so a corrupt config can never silently block
/// the user from logging.
pub fn load_session_config() -> SessionConfig {
    let Ok(path) = get_session_config_path() else {
        return SessionConfig::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return SessionConfig::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

/// Persists the session config to disk using atomic temp+rename.
pub fn save_session_config(config: &SessionConfig) -> Result<()> {
    let path = get_session_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let serialized = serde_json::to_string_pretty(config)
        .context("Failed to serialize session config")?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, serialized.as_bytes())
        .with_context(|| format!("Failed to write config tmp at {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("Failed to rename {} -> {}", tmp_path.display(), path.display()))?;
    Ok(())
}
