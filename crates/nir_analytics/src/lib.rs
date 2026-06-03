use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AnalyticsState {
    pub processed_files: std::collections::HashMap<String, usize>,
    pub retry_count: usize,
    pub failed_at_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Resolves the state file path.
fn get_state_path() -> Result<PathBuf> {
    let mut logs_dir = get_logs_dir()?;
    logs_dir.push("state.json");
    Ok(logs_dir)
}

/// Loads the current analytics state from disk.
pub fn load_analytics_state() -> Result<AnalyticsState> {
    let path = get_state_path()?;
    if !path.exists() {
        return Ok(AnalyticsState::default());
    }
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let state = serde_json::from_str(&content).unwrap_or_default();
    Ok(state)
}

/// Saves the current analytics state to disk.
pub fn save_analytics_state(state: &AnalyticsState) -> Result<()> {
    let path = get_state_path()?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let serialized = serde_json::to_string_pretty(state).context("Failed to serialize analytics state")?;
    file.write_all(serialized.as_bytes())?;
    Ok(())
}

/// Counts unprocessed entries.
pub fn count_unprocessed_entries(state: &AnalyticsState) -> Result<usize> {
    let logs_dir = get_logs_dir()?;
    let mut unprocessed_count = 0;

    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                let processed_lines = state.processed_files.get(filename).cloned().unwrap_or(0);
                let total_entries = count_file_lines(&path).unwrap_or(0);

                if total_entries > processed_lines {
                    unprocessed_count += total_entries - processed_lines;
                }
            }
        }
    }

    Ok(unprocessed_count)
}

/// Counts valid log entries: lines whose first non-whitespace character is `[`.
/// Blank lines, trailing whitespace, headers, and malformed rows are excluded so
/// the processed watermark tracks real entries instead of raw newline count.
fn count_file_lines(path: &std::path::Path) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let count = content
        .lines()
        .filter(|line| line.trim_start().starts_with('['))
        .count();
    Ok(count)
}

/// Returns true if a background scan should be initiated.
pub fn should_execute_analysis(_last_foreground_success: Option<DateTime<Utc>>) -> Result<bool> {
    Ok(true)
}

/// Updates processed watermarks for all log files.
pub fn update_state_watermarks() -> Result<()> {
    let mut state = load_analytics_state()?;
    let logs_dir = get_logs_dir()?;

    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                let actual_entries = count_file_lines(&path).unwrap_or(0);
                state.processed_files.insert(filename.to_string(), actual_entries);
            }
        }
    }

    state.retry_count = 0;
    state.failed_at_timestamp = None;

    save_analytics_state(&state)?;
    Ok(())
}

/// Marks a scan attempt as failed.
pub fn mark_analysis_failure() -> Result<()> {
    let mut state = load_analytics_state()?;
    state.retry_count += 1;
    state.failed_at_timestamp = Some(Utc::now());
    save_analytics_state(&state)?;
    Ok(())
}

/// Collects unread log lines across all daily logs. Returns entries in
/// `LogEntry` form so callers get the timestamp/id/content split for free.
/// Warns (once per file) when a non-empty line doesn't match the expected
/// `[HH:MM:SS] | ID:...` format so silent log-format drift is visible.
pub fn collect_unprocessed_log_lines(limit: usize) -> Result<Vec<String>> {
    let state = load_analytics_state()?;
    let logs_dir = get_logs_dir()?;
    let mut collected = Vec::new();
    
    let files = list_log_files()?;
    
    for filename in files.iter().rev() {
        if collected.len() >= limit {
            break;
        }
        
        let file_path = logs_dir.join(filename);
        let processed_lines = state.processed_files.get(filename).cloned().unwrap_or(0);
        
        let mut file = OpenOptions::new().read(true).open(&file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        
        // Skip past header lines: a header is anything before the first `[`
        // line. This handles files with/without the 2-line `# Daily Log` header
        // uniformly.
        let lines: Vec<&str> = content.lines().collect();
        let first_data_index = lines
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .unwrap_or(lines.len());
        let start_index = first_data_index + processed_lines;
        
        let mut malformed_warned = false;
        for line in lines.iter().skip(start_index) {
            if collected.len() >= limit {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if parse_line_entry(trimmed).is_none() && !malformed_warned {
                log::warn!(
                    "{}: skipping non-canonical log line: {:?}",
                    filename, trimmed
                );
                malformed_warned = true;
            }
            collected.push(trimmed.to_string());
        }
    }
    
    Ok(collected)
}

/// Generates the prompt used to detect recurring patterns.
pub fn generate_analysis_prompt(logs: &[String]) -> String {
    let logs_serialized = logs.join("\n");
    format!(
r##"You are a background pattern analyzer for the /nir development environment. 
You are given a slice of user task telemetry records. Your sole objective is to detect repetitive workflows or recurring tasks that have occurred 3 or more times across these sessions.

If a clear pattern is identified, synthesize an autonomous tool following the SKILL.md specification. Your output MUST be a strict JSON object matching the schema below. If no distinct repetitive workflow pattern exists, return an empty JSON object: {{}}

CRITICAL RULES:
1. Do not output markdown code blocks (e.g., do not wrap in ```json), text headers, or any conversational prose. Output raw valid JSON only.
2. The generated skill actions must be generalized instructions or prompt guidelines that help the agent complete this specific recurring task automatically when triggered.

OUTPUT JSON SCHEMA:
{{
    "pattern_found": true,
    "skill_name": "lowercase-kebab-case-name",
    "description": "Short clear summary of what workflow this skill automates",
    "skill_markdown": "# Skill Name\n\n## Description\n...\n\n## Instructions\n..."
}}

TELEMETRY INPUT LOGS:
{}"##,
        logs_serialized
    )
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnalysisResponse {
    pattern_found: bool,
    skill_name: Option<String>,
    description: Option<String>,
    skill_markdown: Option<String>,
}

/// Resolves the staging directory for proposed skills.
fn get_skills_dir() -> Result<PathBuf> {
    let mut home = resolve_home_dir();
    home.push(".agents");
    home.push("proposals");
    fs::create_dir_all(&home).context("Failed to create skills staging directory")?;
    Ok(home)
}

/// Processes the model's pattern analysis response.
pub fn process_analysis_response(raw_json: &str) -> Result<Option<String>> {
    let trimmed = raw_json.trim();
    
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');
    
    let clean_json = match (start, end) {
        (Some(s), Some(e)) if e > s => &trimmed[s..=e],
        _ => trimmed,
    };

    let normalized = clean_json.replace(" ", "").replace("\n", "").replace("\r", "");
    if normalized == "{}" {
        update_state_watermarks()?;
        return Ok(None);
    }

    let parsed: AnalysisResponse = match serde_json::from_str(clean_json) {
        Ok(res) => res,
        Err(_) => {
            mark_analysis_failure()?;
            return Err(anyhow::anyhow!("Model output failed to map cleanly to strict analysis schemas"));
        }
    };

    if !parsed.pattern_found {
        update_state_watermarks()?;
        return Ok(None);
    }

    let skill_name = parsed.skill_name.context("JSON execution missing skill_name field")?;
    let description = parsed.description.unwrap_or_else(|| "Automated workspace skill".to_string());
    let skill_markdown = parsed.skill_markdown.context("JSON execution missing skill_markdown field")?;
    let sanitized_name = skill_name
        .trim()
        .to_lowercase()
        .replace(' ', "-")
        .replace('_', "-");
    if sanitized_name.is_empty() {
        return Err(anyhow::anyhow!("Extracted skill name evaluation resulted in empty string parameters"));
    }

    let mut target_proposal_dir = get_skills_dir()?;
    target_proposal_dir.push(&sanitized_name);
    fs::create_dir_all(&target_proposal_dir).context("Failed to provision individual proposal directories")?;

    let markdown_file_path = target_proposal_dir.join("SKILL.md");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(markdown_file_path)
        .context("Failed to open compiled SKILL.md target frame")?;

    let structured_payload = format!(
r#"---
name: {}
description: {}
status: proposal
---
{}"#,
        sanitized_name, description, skill_markdown
    );

    file.write_all(structured_payload.as_bytes()).context("Failed to commit formatted tool contents to storage disk")?;

    update_state_watermarks()?;

    let home = resolve_home_dir();
    let index_path = home.join(".nir/brain/skills_index.json");

    if !index_path.exists() {
        return Ok(Some(sanitized_name));
    }

    let index_content = fs::read_to_string(&index_path)
        .context("Failed to read skills_index.json")?;
    let mut json_val: serde_json::Value = serde_json::from_str(&index_content)
        .context("Failed to parse skills_index.json")?;

    let is_similar = |existing: &str, new_slug: &str| -> bool {
        if existing == new_slug {
            return true;
        }
        let existing_stems: std::collections::HashSet<String> = existing
            .split('-')
            .map(|w| w.chars().take(4).collect::<String>())
            .collect();
        let new_stems: std::collections::HashSet<String> = new_slug
            .split('-')
            .map(|w| w.chars().take(4).collect::<String>())
            .collect();
        existing_stems.intersection(&new_stems).count() >= 2
    };

    let already_indexed = json_val
        .get("active_skills")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .chain(
            json_val
                .get("discovered_skills")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten(),
        )
        .any(|s| {
            s.get("name")
                .and_then(|n| n.as_str())
                .map_or(false, |name| is_similar(name, &sanitized_name))
        });

    if !already_indexed {
        let discovered = json_val
            .get_mut("discovered_skills")
            .and_then(|v| v.as_array_mut())
            .context("discovered_skills array missing from skills_index.json")?;

        discovered.push(serde_json::json!({
            "name": sanitized_name,
            "description": description,
            "last_used_timestamp": Utc::now().timestamp(),
        }));

        let updated_json = serde_json::to_string_pretty(&json_val)
            .context("Failed to serialize skills_index.json")?;
        fs::write(&index_path, updated_json)
            .context("Failed to write skills_index.json")?;
    }

    Ok(Some(sanitized_name))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StagedProposal {
    pub slug: String,
    pub name: String,
    pub description: String,
}

/// Lists all staged skills proposals.
pub fn list_staged_proposals() -> Result<Vec<StagedProposal>> {
    let proposals_dir = get_skills_dir()?;
    let mut list = Vec::new();

    if !proposals_dir.exists() {
        return Ok(list);
    }

    for entry in fs::read_dir(proposals_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let skill_md_path = path.join("SKILL.md");
            if skill_md_path.exists() {
                if let Some(slug) = path.file_name().and_then(|s| s.to_str()) {
                    let content = fs::read_to_string(&skill_md_path)?;
                    
                    let mut name = slug.to_string();
                    let mut description = String::new();
                    
                    let lines: Vec<&str> = content.lines().collect();
                    if lines.first().map_or(false, |l| l.trim() == "---") {
                        for line in lines.iter().skip(1) {
                            if line.trim() == "---" { break; }
                            if let Some(idx) = line.find(':') {
                                let key = line[..idx].trim();
                                let val = line[idx + 1..].trim();
                                if key == "name" { name = val.to_string(); }
                                if key == "description" { description = val.to_string(); }
                            }
                        }
                    }

                    list.push(StagedProposal {
                        slug: slug.to_string(),
                        name,
                        description,
                    });
                }
            }
        }
    }

    Ok(list)
}

/// Approves a staged proposal and moves it to active global skills.
pub fn approve_staged_skill(slug: &str) -> Result<()> {
    let proposals_dir = get_skills_dir()?;
    let source_path = proposals_dir.join(slug);

    if !source_path.exists() {
        return Err(anyhow::anyhow!("Target proposal folder does not exist"));
    }

    let mut destination_dir = resolve_home_dir();
    destination_dir.push(".agents");
    destination_dir.push("skills");
    fs::create_dir_all(&destination_dir)?;

    let target_path = destination_dir.join(slug);

    if target_path.exists() {
        fs::remove_dir_all(&target_path)?;
    }

    fs::rename(source_path, target_path).context("Failed to shift proposal to active skills directory")?;
    Ok(())
}

/// Deletes a staged proposal from disk.
pub fn reject_staged_skill(slug: &str) -> Result<()> {
    let proposals_dir = get_skills_dir()?;
    let target_path = proposals_dir.join(slug);

    if target_path.exists() && target_path.is_dir() {
        fs::remove_dir_all(target_path).context("Failed to scrub rejected proposal folder from filesystem")?;
    }
    Ok(())
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
// Two-tiered hybrid similarity gate
// =============================================================================

/// Routing decision returned by `evaluate_match`.
pub enum MatchResult {
    DirectMerge(String),
    RequiresReflection(String),
    NoMatch,
}

/// Routes a candidate summary against an existing cluster using two-tiered
/// overlap thresholds.
pub fn evaluate_match(candidate_summary: &str, existing_cluster_summary: &str) -> MatchResult {
    let score = overlap_coefficient(candidate_summary, existing_cluster_summary);
    if score >= 0.40 {
        MatchResult::DirectMerge(existing_cluster_summary.to_string())
    } else if score >= 0.10 {
        MatchResult::RequiresReflection(existing_cluster_summary.to_string())
    } else {
        MatchResult::NoMatch
    }
}

/// Overlap coefficient (Szymkiewicz-Simpson): intersection / min(|A|, |B|).
/// Filters action verbs so scoring is driven by nouns/identifiers.
pub fn overlap_coefficient(first: &str, second: &str) -> f32 {
    let stop_words: &HashSet<&str> = &[
        "the", "and", "for", "with", "from", "this", "that", "into", "were",
        // Action verbs that appear in nearly every log line
        "created", "updated", "fixed", "added", "removed", "implemented",
        "refactored", "optimized", "deployed", "shipped", "completed",
        "started", "finished", "working", "task", "work", "code", "setup",
        "using", "based", "file", "also",
    ].iter().copied().collect();

    let clean_tokens = |source: &str| -> HashSet<String> {
        source
            .to_lowercase()
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.len() > 2 && !stop_words.contains(*token))
            .map(|token| token.to_string())
            .collect()
    };

    let first_tokens = clean_tokens(first);
    let second_tokens = clean_tokens(second);

    if first_tokens.is_empty() || second_tokens.is_empty() {
        return 0.0;
    }

    let intersection_count = first_tokens.intersection(&second_tokens).count();
    let minimum_size = std::cmp::min(first_tokens.len(), second_tokens.len());
    intersection_count as f32 / minimum_size as f32
}

#[cfg(test)]
mod fts_and_gate_tests {
    use super::*;

    #[test]
    fn overlap_full_overlap() {
        // Set A: { "apple", "banana", "cherry" } (size 3)
        // Set B: { "apple", "banana" }         (size 2)
        // intersection: 2, min: 2, score: 1.0
        let score = overlap_coefficient("apple banana cherry", "apple banana");
        assert!((score - 1.0).abs() < f32::EPSILON, "Expected 1.0, got {}", score);
    }

    #[test]
    fn overlap_partial_overlap() {
        // Set A: { "apple", "banana", "cherry", "date" }  (size 4)
        // Set B: { "apple", "banana", "elderberry" }      (size 3)
        // intersection: 2, min: 3, score: 0.6667
        let score = overlap_coefficient("apple banana cherry date", "apple banana elderberry");
        assert!((score - 0.6666667).abs() < 0.001, "Expected ~0.667, got {}", score);
    }

    #[test]
    fn overlap_empty_input_returns_zero() {
        assert_eq!(overlap_coefficient("", "apple"), 0.0);
        assert_eq!(overlap_coefficient("apple", ""), 0.0);
        assert_eq!(overlap_coefficient("", ""), 0.0);
    }

    #[test]
    fn evaluate_match_direct_merge_at_high_overlap() {
        // Set A: { alpha, beta, charlie, delta, echo }    (size 5)
        // Set B: { alpha, beta, foxtrot, golf, hotel }    (size 5)
        // intersection: 2, min: 5, score: 0.40 -> DirectMerge (>= 0.40)
        match evaluate_match(
            "alpha beta charlie delta echo",
            "alpha beta foxtrot golf hotel",
        ) {
            MatchResult::DirectMerge(_) => (),
            other => panic!("Expected DirectMerge at 0.40 boundary, got {:?}", other),
        }
    }

    #[test]
    fn evaluate_match_reflection_in_gray_zone() {
        // Set A: 10 distinct tokens, 1 shared with cluster -> 1/10 = 0.10 -> RequiresReflection
        let candidate =
            "alpha bravo charlie delta echo foxtrot golf hotel india juliet";
        let cluster =
            "alpha mike november oscar papa quebec romeo sierra tango uniform";
        match evaluate_match(candidate, cluster) {
            MatchResult::RequiresReflection(_) => (),
            other => panic!("Expected RequiresReflection, got {:?}", other),
        }
    }

    #[test]
    fn evaluate_match_no_match_below_threshold() {
        // No shared tokens -> score 0.0 -> NoMatch
        match evaluate_match("alpha beta charlie", "delta echo foxtrot") {
            MatchResult::NoMatch => (),
            other => panic!("Expected NoMatch, got {:?}", other),
        }
    }
}
