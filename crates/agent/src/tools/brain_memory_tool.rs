use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;


use gpui::{App, Entity, SharedString, Task};
use project::Project;
use agent_client_protocol::schema as acp;

use crate::{AgentTool, ToolCallEventStream, ToolInput};

const WORKING_NOTES_SECTION: &str = "## 📝 Working Notes";

pub fn brain_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string())
    } else {
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    };
    PathBuf::from(base).join(".nir").join("brain")
}

pub fn memory_dir() -> PathBuf {
    brain_dir().join("memory")
}

pub fn backup_dir() -> PathBuf {
    brain_dir().join("memory").join(".backups")
}

fn get_today_date() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn get_clock_time() -> String {
    Utc::now().format("%H:%M:%S").to_string()
}

fn is_valid_iso_date(date: &str) -> bool {
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
    re.is_match(date)
}

fn build_entry_id(date: &str) -> String {
    let compact = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let random: u32 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() % 10000;
    format!("{}-{}-{:04}", date, compact, random)
}


fn ensure_dir(dir_path: &Path) {
    if !dir_path.exists() {
        let _ = fs::create_dir_all(dir_path);
    }
}

fn ensure_bookmark_file(file_path: &Path) {
    if !file_path.exists() {
        let content = format!("# 📌 Bookmarks & Ideas\n\n> ⚠️ AGENT RULE: Append only. Never rewrite this file. Use brain_memory with target=bookmark.\n\n## 💡 Ideas\n\n## 🔗 Links\n\n## 📝 Prompts & Tests\n\n## 🧪 Things to Try\n\n{}\n\n", WORKING_NOTES_SECTION);
        let _ = fs::write(file_path, content);
    }
}

fn auto_backup(file_path: &Path) {
    if !file_path.exists() {
        return;
    }
    let b_dir = backup_dir();
    ensure_dir(&b_dir);
    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy();
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let backup_path = b_dir.join(format!("{}.{}.bak", file_name, timestamp));
    if let Ok(_) = fs::copy(file_path, &backup_path) {
        if let Ok(entries) = fs::read_dir(&b_dir) {
            let mut backups: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.starts_with(file_name.as_ref()) {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect();
            backups.sort();
            backups.reverse();
            for old in backups.into_iter().skip(5) {
                let _ = fs::remove_file(b_dir.join(old));
            }
        }
    }
}

fn safe_write(file_path: &Path, content: &str) -> Result<(), String> {
    auto_backup(file_path);
    fs::write(file_path, content).map_err(|e| format!("Write failed: {}", e))?;
    let actual = fs::read_to_string(file_path).map_err(|_| "Verify failed".to_string())?;
    if actual != content {
        return Err("Verification failed after write. Backup exists.".to_string());
    }
    Ok(())
}

fn ensure_working_notes_section(file_path: &Path, target_name: &str) -> Result<String, String> {
    if !file_path.exists() {
        if target_name == "bookmark" {
            ensure_bookmark_file(file_path);
        } else {
            let content = format!("# {}\n\n## 📝 Working Notes\n\n", target_name.to_uppercase());
            let _ = fs::write(file_path, content);
        }
    }
    let mut content = fs::read_to_string(file_path).unwrap_or_default();
    if !content.contains(WORKING_NOTES_SECTION) {
        let separator = if content.ends_with("\n\n") {
            ""
        } else if content.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        content = format!("{}{}{}\n\n", content, separator, WORKING_NOTES_SECTION);
        safe_write(file_path, &content)?;
    }
    fs::read_to_string(file_path).map_err(|e| e.to_string())
}

struct BlockParts {
    before: String,
    section: String,
    after: String,
}

fn extract_working_notes_block(content: &str) -> BlockParts {
    if let Some(section_idx) = content.find(WORKING_NOTES_SECTION) {
        let after_header = &content[section_idx + WORKING_NOTES_SECTION.len()..];
        let next_section_regex = Regex::new(r"\n## ").unwrap();
        let next_section_offset = next_section_regex
            .find(after_header)
            .map(|m| m.start())
            .unwrap_or(after_header.len());

        BlockParts {
            before: content[..section_idx].to_string(),
            section: format!("{}{}", WORKING_NOTES_SECTION, &after_header[..next_section_offset]),
            after: after_header[next_section_offset..].to_string(),
        }
    } else {
        BlockParts {
            before: content.to_string(),
            section: format!("{}\n\n", WORKING_NOTES_SECTION),
            after: "".to_string(),
        }
    }
}

fn build_entry(date: &str, id: &str, content: &str) -> String {
    format!("### [{}] {} | ID: {}\n{}\n\n---\n\n", get_clock_time(), date, id, content.trim())
}

struct ParsedEntry {
    id: String,
    date: String,
    time: String,
    content: String,
    updated_time: Option<String>,
}

fn list_entries(section: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = section.lines().collect();
    let header_re = Regex::new(r"^### \[([^\]]+)\] (\d{4}-\d{2}-\d{2}) \| ID: ([a-zA-Z0-9_-]+)(?:\s+\[updated\s+([^\]]+)\])?\s*$").unwrap();
    
    let mut current_entry: Option<ParsedEntry> = None;
    let mut current_content = Vec::new();
    
    for line in lines {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("### [") {
            if let Some(cap) = header_re.captures(trimmed_line) {
                if let Some(entry) = current_entry.take() {
                    let content_str = current_content.join("\n");
                    let mut trimmed = content_str.trim().to_string();
                    if trimmed.ends_with("\n---") {
                        trimmed.truncate(trimmed.len() - 4);
                    } else if trimmed == "---" {
                        trimmed.clear();
                    }
                    entries.push(ParsedEntry {
                        time: entry.time,
                        date: entry.date,
                        id: entry.id,
                        content: trimmed.trim().to_string(),
                        updated_time: entry.updated_time,
                    });
                    current_content.clear();
                }
                
                current_entry = Some(ParsedEntry {
                    time: cap[1].trim().to_string(),
                    date: cap[2].trim().to_string(),
                    id: cap[3].trim().to_string(),
                    content: String::new(),
                    updated_time: cap.get(4).map(|m| m.as_str().to_string()),
                });
                continue;
            }
        }
        
        if current_entry.is_some() {
            current_content.push(line);
        }
    }
    
    if let Some(entry) = current_entry {
        let content_str = current_content.join("\n");
        let mut trimmed = content_str.trim().to_string();
        if trimmed.ends_with("\n---") {
            trimmed.truncate(trimmed.len() - 4);
        } else if trimmed == "---" {
            trimmed.clear();
        }
        entries.push(ParsedEntry {
            time: entry.time,
            date: entry.date,
            id: entry.id,
            content: trimmed.trim().to_string(),
            updated_time: entry.updated_time,
        });
    }
    
    entries
}

fn serialize_entries(entries: &[ParsedEntry]) -> String {
    let mut out = String::new();
    out.push_str(WORKING_NOTES_SECTION);
    out.push_str("\n\n");
    for e in entries {
        let updated_suffix = if let Some(ref ut) = e.updated_time {
            format!(" [updated {}]", ut)
        } else {
            "".to_string()
        };
        out.push_str(&format!("### [{}] {} | ID: {}{}\n{}\n\n---\n\n", e.time, e.date, e.id, updated_suffix, e.content.trim()));
    }
    out
}

fn is_duplicate_entry(entries: &[ParsedEntry], target_date: &str, new_content: &str) -> bool {
    let snippet = new_content.trim().chars().take(60).collect::<String>().to_lowercase();
    entries.iter().any(|e| {
        e.date == target_date && e.content.chars().take(60).collect::<String>().to_lowercase() == snippet
    })
}

fn read_full_file(file_path: &Path, target_name: &str) -> String {
    if !file_path.exists() {
        if target_name == "bookmark" {
            ensure_bookmark_file(file_path);
        } else {
            let content = format!("# {}\n\n## 📝 Working Notes\n\n", target_name.to_uppercase());
            let _ = fs::write(file_path, content);
        }
    }
    fs::read_to_string(file_path).unwrap_or_else(|_| format!("[{}: file read error]", target_name))
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemoryAction {
    #[default]
    Auto,
    Create,
    Read,
    ReadMany,
    ReadAll,
    Modify,
    Delete,
    List,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BrainMemoryTarget {
    #[default]
    Auto,
    About,
    Goals,
    Settings,
    Projects,
    Bookmark,
}

impl BrainMemoryTarget {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::About => "about",
            Self::Goals => "goals",
            Self::Settings => "settings",
            Self::Projects => "projects",
            Self::Bookmark => "bookmark",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrainMemoryInput {
    /// Action to perform:
    /// - auto: Infer from content
    /// - create: Add a new entry
    /// - read: Retrieve entries
    /// - read-many: Batch read files
    /// - read-all: Dump all memory
    /// - list: Index entries
    /// - modify: Update existing entry
    /// - delete: Remove entry
    #[serde(default)]
    action: BrainMemoryAction,
    /// Destination file target:
    /// - auto: Defaults to 'settings'
    /// - about: Identity/Personal info
    /// - goals: Objectives/Milestones
    /// - settings: System configurations
    /// - projects: Active work tracking
    /// - bookmark: Quick links and ideas
    #[serde(default)]
    target: BrainMemoryTarget,
    /// Comma-separated list of targets (for ReadMany).
    targets: Option<String>,
    /// The content to write or modify.
    content: Option<String>,
    /// The date of the entry to modify or delete.
    date: Option<String>,
    /// The ID of the entry to modify or delete.
    id: Option<String>,
}

pub struct BrainMemoryTool {
    _project: Entity<Project>,
}

impl BrainMemoryTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { _project: project }
    }
}

impl AgentTool for BrainMemoryTool {
    type Input = BrainMemoryInput;
    type Output = String;

    const NAME: &'static str = "brain_memory";

    fn description() -> SharedString {
        "Persistent memory across sessions. Stores notes in 5 files: about, goals, settings, projects, bookmark.

AVAILABLE ACTIONS:
- `auto`: Infer action from content (default)
- `create`: Add a new entry to a target file
- `read`: Retrieve entries by ID, date, or all from a target
- `read-many`: Batch read multiple target files (comma-separated targets)
- `read-all`: Dump all 5 memory files at once
- `list`: Index entries in a target file
- `modify`: Update an existing entry by ID
- `delete`: Remove an entry by ID

WHEN TO USE:
- Persisting user preferences, identity, or project context across sessions
- Logging milestones, goals, or completed tasks
- Storing bookmarks, links, or ideas the user wants to keep
- Recording agent configuration or tool settings

WHEN NOT TO USE:
- For session-scoped working notes — use `scratchpad` instead
- For one-off information that doesn't need to persist
- Do NOT delete without reading first; deletions are by ID and irreversible

TARGETS:
- `auto`: Defaults to `settings` (default for create with no target)
- `about`: Identity & personal info
- `goals`: Objectives & milestones
- `settings`: System configurations
- `projects`: Active work tracking
- `bookmark`: Links, ideas, things to try

Backups: every write auto-saves to `~/.nir/brain/memory/.backups/` (last 5 kept).".into()
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        let Ok(input) = input else {
            return "brain_memory".into();
        };
        let action = serde_json::to_value(input.action)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "auto".to_string());
        let mut parts = vec![format!("action={}", action)];
        if let Some(targets) = input.targets.as_deref() {
            parts.push(format!("targets={}", targets));
        } else if input.target.as_str() != "auto" {
            parts.push(format!("target={}", input.target.as_str()));
        }
        parts.join(" ").into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        cx.spawn(async move |_cx| {
            let input = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;

            let mut action = input.action;
            if action == BrainMemoryAction::Auto {
                action = if input.content.as_ref().map(|c| !c.trim().is_empty()).unwrap_or(false) {
                    BrainMemoryAction::Create
                } else {
                    BrainMemoryAction::Read
                };
            }

            if let Some(ref date) = input.date {
                if !is_valid_iso_date(date) {
                    return Err("Error: date must be in YYYY-MM-DD format.".to_string());
                }
            }

            let target_date = input.date.clone().unwrap_or_else(|| get_today_date());
            let target_name = if input.target.as_str() != "auto" {
                input.target.as_str()
            } else {
                "settings"
            };

            let m_dir = memory_dir();
            ensure_dir(&m_dir);

            let target_file = m_dir.join(format!("{}.md", target_name));

            if action == BrainMemoryAction::ReadAll {
                let all_targets = ["about", "goals", "settings", "projects", "bookmark"];
                let mut results = Vec::new();
                for t in all_targets {
                    let fp = m_dir.join(format!("{}.md", t));
                    let content = read_full_file(&fp, t);
                    results.push(format!("{0}\n# {1}\n{0}\n{2}", "=".repeat(40), t.to_uppercase(), content));
                }
                return Ok(results.join("\n\n"));
            }

            if action == BrainMemoryAction::ReadMany {
                let t_str = input.targets.ok_or_else(|| "Error: 'targets' is required for read-many.".to_string())?;
                let valid_set: HashSet<&str> = ["about", "goals", "settings", "projects", "bookmark"].into_iter().collect();
                let requested: Vec<&str> = t_str.split(',')
                    .map(|s| s.trim())
                    .filter(|s| valid_set.contains(s))
                    .collect();
                
                if requested.is_empty() {
                    return Err(format!("Error: no valid targets found in '{}'. Valid: about, goals, settings, projects, bookmark.", t_str));
                }

                let mut results = Vec::new();
                for t in requested {
                    let fp = m_dir.join(format!("{}.md", t));
                    let content = read_full_file(&fp, t);
                    results.push(format!("{0}\n# {1}\n{0}\n{2}", "=".repeat(40), t.to_uppercase(), content));
                }
                return Ok(results.join("\n\n"));
            }

            if action == BrainMemoryAction::List {
                let content = ensure_working_notes_section(&target_file, target_name)?;
                let parts = extract_working_notes_block(&content);
                let entries = list_entries(&parts.section);
                if entries.is_empty() {
                    return Ok(format!("No working notes in {}.md.", target_name));
                }
                let lines: Vec<String> = entries.iter().enumerate().map(|(i, e)| {
                    let first_line = e.content.lines().next().unwrap_or("");
                    let updated_suffix = if let Some(ref ut) = e.updated_time {
                        format!(" [updated {}]", ut)
                    } else {
                        "".to_string()
                    };
                    format!("{}. [{}] {} | ID: {}{}\n   {}", i + 1, e.time, e.date, e.id, updated_suffix, first_line)
                }).collect();
                return Ok(format!("Working Notes in {}.md ({}):\n\n{}", target_name, entries.len(), lines.join("\n\n")));
            }

            if action == BrainMemoryAction::Create {
                let text = input.content.ok_or_else(|| "Error: content is required for 'create'.".to_string())?;
                if text.trim().is_empty() {
                    return Err("Error: content is empty.".to_string());
                }
                let content = ensure_working_notes_section(&target_file, target_name)?;
                let parts = extract_working_notes_block(&content);
                let entries = list_entries(&parts.section);

                if is_duplicate_entry(&entries, &target_date, &text) {
                    return Ok(format!("Skipped: similar note already exists in {}.md for {}. No duplicate written.", target_name, target_date));
                }

                let entry_id = build_entry_id(&target_date);
                let new_entry = build_entry(&target_date, &entry_id, &text);
                let updated_section = format!("{}\n\n{}", parts.section.trim_end(), new_entry);
                safe_write(&target_file, &format!("{}{}{}", parts.before, updated_section, parts.after))?;
                return Ok(format!("Working note created in {}.md with ID {}.", target_name, entry_id));
            }

            if action == BrainMemoryAction::Read {
                let content = ensure_working_notes_section(&target_file, target_name)?;
                let parts = extract_working_notes_block(&content);
                let entries = list_entries(&parts.section);
                if entries.is_empty() {
                    return Ok(format!("No working notes in {}.md.", target_name));
                }

                if let Some(id) = input.id {
                    if let Some(found) = entries.iter().find(|e| e.id == id) {
                        let updated_suffix = if let Some(ref ut) = found.updated_time {
                            format!(" [updated {}]", ut)
                        } else {
                            "".to_string()
                        };
                        return Ok(format!("[{}] {} | ID: {}{}\n{}", found.time, found.date, found.id, updated_suffix, found.content));
                    }
                    return Err(format!("No working note found with ID {} in {}.md.", id, target_name));
                }

                if let Some(date) = input.date {
                    let filtered: Vec<&ParsedEntry> = entries.iter().filter(|e| e.date == date).collect();
                    if filtered.is_empty() {
                        return Ok(format!("No working notes for {} in {}.md.", date, target_name));
                    }
                    let res: Vec<String> = filtered.iter().map(|e| {
                        let updated_suffix = if let Some(ref ut) = e.updated_time {
                            format!(" [updated {}]", ut)
                        } else {
                            "".to_string()
                        };
                        format!("[{}] {} | ID: {}{}\n{}", e.time, e.date, e.id, updated_suffix, e.content)
                    }).collect();
                    return Ok(res.join("\n\n---\n\n"));
                }

                let res: Vec<String> = entries.iter().map(|e| {
                    let updated_suffix = if let Some(ref ut) = e.updated_time {
                        format!(" [updated {}]", ut)
                    } else {
                        "".to_string()
                    };
                    format!("[{}] {} | ID: {}{}\n{}", e.time, e.date, e.id, updated_suffix, e.content)
                }).collect();
                return Ok(res.join("\n\n---\n\n"));
            }

            if action == BrainMemoryAction::Modify {
                let id = input.id.ok_or_else(|| "Error: id is required for 'modify'.".to_string())?;
                let text = input.content.ok_or_else(|| "Error: content is required for 'modify'.".to_string())?;
                if text.trim().is_empty() {
                    return Err("Error: content is empty.".to_string());
                }

                let content = ensure_working_notes_section(&target_file, target_name)?;
                let parts = extract_working_notes_block(&content);
                let mut entries = list_entries(&parts.section);
                
                if let Some(target) = entries.iter_mut().find(|e| e.id == id) {
                    target.content = text.trim().to_string();
                    target.updated_time = Some(get_clock_time());
                    
                    let updated_section = serialize_entries(&entries);
                    safe_write(&target_file, &format!("{}{}{}", parts.before, updated_section, parts.after))?;
                    return Ok(format!("Working note {} updated in {}.md.", id, target_name));
                } else {
                    return Err(format!("No working note found with ID {} in {}.md.", id, target_name));
                }
            }

            if action == BrainMemoryAction::Delete {
                let id = input.id.ok_or_else(|| "Error: id is required for 'delete'.".to_string())?;
                
                let content = ensure_working_notes_section(&target_file, target_name)?;
                let parts = extract_working_notes_block(&content);
                let mut entries = list_entries(&parts.section);
                
                if entries.iter().any(|e| e.id == id) {
                    entries.retain(|e| e.id != id);
                    let updated_section = serialize_entries(&entries);
                    safe_write(&target_file, &format!("{}{}{}", parts.before, updated_section, parts.after))?;
                    return Ok(format!("Deleted working note {} from {}.md.", id, target_name));
                } else {
                    return Err(format!("No working note found with ID {} in {}.md.", id, target_name));
                }
            }

            Err("Invalid action.".to_string())
        })
    }
}
