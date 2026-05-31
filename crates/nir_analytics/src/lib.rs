use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub content: String,
}

/// Resolves the logs directory path.
fn get_logs_dir() -> Result<PathBuf> {
    let mut home = match std::env::var("HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .context("Failed to resolve home directory")?,
    };
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
                let total_lines = count_file_lines(&path).unwrap_or(0);
                
                let actual_total_entries = if total_lines >= 2 { total_lines - 2 } else { 0 };
                
                if actual_total_entries > processed_lines {
                    unprocessed_count += actual_total_entries - processed_lines;
                }
            }
        }
    }

    Ok(unprocessed_count)
}

/// Helper to count newlines in a file.
fn count_file_lines(path: &std::path::Path) -> Result<usize> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut count = 0;
    let mut buffer = [0; 8192];

    while let Ok(bytes_read) = reader.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }
        count += buffer[..bytes_read].iter().filter(|&&byte| byte == b'\n').count();
    }

    Ok(count)
}

/// Returns true if a background scan should be initiated.
pub fn should_execute_analysis(last_foreground_success: Option<DateTime<Utc>>) -> Result<bool> {
    let state = load_analytics_state()?;
    
    let unprocessed = count_unprocessed_entries(&state)?;
    if unprocessed < 15 {
        return Ok(false);
    }

    if let Some(failed_time) = state.failed_at_timestamp {
        let api_is_actively_working = match last_foreground_success {
            Some(last_success) => last_success > failed_time,
            None => false,
        };

        if !api_is_actively_working {
            let duration_since_failure = Utc::now().signed_duration_since(failed_time);
            if duration_since_failure.num_hours() < 2 {
                return Ok(false);
            }
        }
    }

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
                let total_lines = count_file_lines(&path).unwrap_or(0);
                let actual_entries = if total_lines >= 2 { total_lines - 2 } else { 0 };
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

/// Collects unread log lines across all daily logs.
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
        
        let lines_to_scan = content.lines().skip(2 + processed_lines);
        for line in lines_to_scan {
            if collected.len() >= limit {
                break;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                collected.push(trimmed.to_string());
            }
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
    let mut home = match std::env::var("HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .context("Failed to resolve home directory")?,
    };
    home.push(".agents");
    home.push("proposals");
    fs::create_dir_all(&home).context("Failed to create skills staging directory")?;
    Ok(home)
}

/// Processes the model's pattern analysis response.
pub fn process_analysis_response(raw_json: &str) -> Result<Option<String>> {
    let parsed: AnalysisResponse = match serde_json::from_str(raw_json.trim()) {
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

    let mut destination_dir = match std::env::var("HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => std::env::var("USERPROFILE").map(PathBuf::from)?
    };
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
