use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use gpui::{App, Entity, SharedString, Task};
use project::Project;
use agent_client_protocol::schema::v1 as acp;

use crate::{AgentTool, ToolCallEventStream, ToolInput};
use super::brain_memory_tool::brain_dir;

const WORKING_NOTES_SECTION: &str = "## 📝 Working Notes";

fn scratchpad_dir() -> PathBuf {
    brain_dir().join("scratch")
}

fn scratch_file() -> PathBuf {
    scratchpad_dir().join("scratchpad.md")
}

fn get_clock_time() -> String {
    Utc::now().format("%H:%M:%S").to_string()
}

fn get_today_date() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn build_entry_id(date: &str) -> String {
    let compact = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let random: u32 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() % 10000;
    format!("{}-{}-{:04}", date, compact, random)
}


fn ensure_scratch_file() -> String {
    let dir = scratchpad_dir();
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    let file_path = scratch_file();
    if !file_path.exists() {
        let content = format!("# 📝 Scratch Pad — Temporary Notes\n\n{}\n\n---\n\n", WORKING_NOTES_SECTION);
        let _ = fs::write(&file_path, content);
    }
    
    let mut content = fs::read_to_string(&file_path).unwrap_or_default();
    if !content.contains(WORKING_NOTES_SECTION) {
        let separator = if content.ends_with("\n\n") { "" } else if content.ends_with('\n') { "\n" } else { "\n\n" };
        content = format!("{}{}{}\n\n---\n\n", content, separator, WORKING_NOTES_SECTION);
        let _ = fs::write(&file_path, &content);
    }
    content
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

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScratchpadAction {
    #[default]
    List,
    Create,
    Read,
    Modify,
    Delete,
    Clear,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScratchpadInput {
    /// The action to perform. Valid actions:
    /// - `list`: List all entries
    /// - `create`: Create a new entry (requires 'content')
    /// - `read`: Read a specific entry (requires 'id') or all entries
    /// - `modify`: Modify an existing entry (requires 'id' and 'content')
    /// - `delete`: Delete an entry (requires 'id')
    /// - `clear`: Clear the entire scratchpad
    #[serde(default)]
    action: ScratchpadAction,
    /// The text content for Create or Modify actions.
    content: Option<String>,
    /// The unique ID of the entry for Read, Modify, or Delete actions.
    id: Option<String>,
}

pub struct ScratchpadTool {
    _project: Entity<Project>,
}

impl ScratchpadTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { _project: project }
    }
}

impl AgentTool for ScratchpadTool {
    type Input = ScratchpadInput;
    type Output = String;

    const NAME: &'static str = "scratchpad";

    fn description() -> SharedString {
        "Your session-level working memory. Use this PROACTIVELY during any multi-step task to stash code snippets, file paths, IDs, search results, partial progress, or anything you'd lose when context scrolls away. Reading is free; forgetting costs tokens. Call `list` first to see what's saved. When in doubt, save it. Use `clear` when starting a fresh task. For permanent cross-session storage use `brain_memory` instead.".into()
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
            return "scratchpad".into();
        };
        let action = serde_json::to_value(input.action)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "list".to_string());
        let mut parts = vec![format!("action={}", action)];
        if let Some(id) = input.id.as_deref() {
            parts.push(format!("id={}", id));
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

            ensure_scratch_file();
            let file_path = scratch_file();
            let content = fs::read_to_string(&file_path).unwrap_or_default();
            let block = extract_working_notes_block(&content);
            let mut entries = list_entries(&block.section);

            match input.action {
                ScratchpadAction::List => {
                    if entries.is_empty() {
                        return Ok("Scratchpad is empty.".to_string());
                    }
                    let lines: Vec<String> = entries.iter().enumerate().map(|(i, e)| {
                        let preview = e.content.lines().next().unwrap_or("No content").to_string();
                        let updated_suffix = if let Some(ref ut) = e.updated_time {
                            format!(" [updated {}]", ut)
                        } else {
                            "".to_string()
                        };
                        format!("{}. [{}] {} | ID: {}{}\n   {}", i + 1, e.time, e.date, e.id, updated_suffix, preview)
                    }).collect();
                    Ok(format!("Scratchpad Entries ({}):\n\n{}", entries.len(), lines.join("\n\n")))
                }
                ScratchpadAction::Create => {
                    let text = input.content.ok_or_else(|| "Error: content required.".to_string())?;
                    let date = get_today_date();
                    let id = build_entry_id(&date);
                    let entry_str = format!("### [{}] {} | ID: {}\n{}\n\n---\n\n", get_clock_time(), date, id, text.trim());
                    let updated_section = format!("{}\n\n{}", block.section.trim_end(), entry_str);
                    let _ = fs::write(&file_path, format!("{}{}{}", block.before, updated_section, block.after));
                    Ok(format!("Scratchpad note saved. ID: {}", id))
                }
                ScratchpadAction::Read => {
                    if let Some(id) = input.id {
                        if let Some(found) = entries.iter().find(|e| e.id == id) {
                            let updated_suffix = if let Some(ref ut) = found.updated_time {
                                format!(" [updated {}]", ut)
                            } else {
                                "".to_string()
                            };
                            Ok(format!("[{}] {} | ID: {}{}\n{}", found.time, found.date, found.id, updated_suffix, found.content))
                        } else {
                            Ok(format!("Note {} not found.", id))
                        }
                    } else {
                        if entries.is_empty() {
                            return Ok("Scratchpad is empty.".to_string());
                        }
                        let text = entries.iter().map(|e| {
                            let updated_suffix = if let Some(ref ut) = e.updated_time {
                                format!(" [updated {}]", ut)
                            } else {
                                "".to_string()
                            };
                            format!("[{}] {} | ID: {}{}\n{}", e.time, e.date, e.id, updated_suffix, e.content)
                        }).collect::<Vec<_>>().join("\n\n---\n\n");
                        Ok(text)
                    }
                }
                ScratchpadAction::Modify => {
                    let id = input.id.ok_or_else(|| "Error: id required for modify.".to_string())?;
                    let text = input.content.ok_or_else(|| "Error: content required for modify.".to_string())?;
                    if let Some(target) = entries.iter_mut().find(|e| e.id == id) {
                        target.content = text.trim().to_string();
                        target.updated_time = Some(get_clock_time());
                        
                        let updated_section = serialize_entries(&entries);
                        let _ = fs::write(&file_path, format!("{}{}{}", block.before, updated_section, block.after));
                        Ok(format!("Checkpoint {} updated.", id))
                    } else {
                        Err(format!("Note {} not found in scratchpad.", id))
                    }
                }
                ScratchpadAction::Delete => {
                    let id = input.id.ok_or_else(|| "Error: id required.".to_string())?;
                    if entries.iter().any(|e| e.id == id) {
                        entries.retain(|e| e.id != id);
                        let updated_section = serialize_entries(&entries);
                        let _ = fs::write(&file_path, format!("{}{}{}", block.before, updated_section, block.after));
                        Ok(format!("Deleted note {}.", id))
                    } else {
                        Err(format!("Note {} not found in scratchpad.", id))
                    }
                }
                ScratchpadAction::Clear => {
                    let updated_section = format!("{}\n\n", WORKING_NOTES_SECTION);
                    let _ = fs::write(&file_path, format!("{}{}{}", block.before, updated_section, block.after));
                    Ok("Scratchpad cleared.".to_string())
                }
            }
        })
    }
}
