use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use gpui::{App, Entity, SharedString, Task};
use project::Project;
use agent_client_protocol::schema as acp;

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
    let random: u32 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() % 10000;
    format!("{}-{}-{:04}", date, compact, random)
}

fn escape_reg_exp(value: &str) -> String {
    regex::escape(value)
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
}

fn list_entries(section: &str) -> Vec<ParsedEntry> {
    let re = Regex::new(r"(?m)### \[([^\]]+)\] (\d{4}-\d{2}-\d{2}) \| ID: ([^\n]+)\n([\s\S]*?)(?:\n---\n|$)").unwrap();
    let mut entries = Vec::new();
    for cap in re.captures_iter(section) {
        entries.push(ParsedEntry {
            time: cap[1].trim().to_string(),
            date: cap[2].trim().to_string(),
            id: cap[3].trim().to_string(),
            content: cap[4].trim().to_string(),
        });
    }
    entries
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
        "Dedicated scratchpad for temporary notes, checkpoints, mid-session context, and raw data dumps. Use for thoughts that don't belong in permanent memory. Use 'modify' to update an existing checkpoint by ID instead of delete+create. NEVER output Working Notes headers.".into()
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Managing scratchpad".into()
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
            let entries = list_entries(&block.section);

            match input.action {
                ScratchpadAction::List => {
                    if entries.is_empty() {
                        return Ok("Scratchpad is empty.".to_string());
                    }
                    let lines: Vec<String> = entries.iter().enumerate().map(|(i, e)| {
                        let preview = e.content.lines().next().unwrap_or("No content").to_string();
                        format!("{}. [{}] {} | ID: {}\n   {}", i + 1, e.time, e.date, e.id, preview)
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
                            Ok(format!("[{}] {} | ID: {}\n{}", found.time, found.date, found.id, found.content))
                        } else {
                            Ok(format!("Note {} not found.", id))
                        }
                    } else {
                        if entries.is_empty() {
                            return Ok("Scratchpad is empty.".to_string());
                        }
                        let text = entries.iter().map(|e| format!("[{}] {} | ID: {}\n{}", e.time, e.date, e.id, e.content)).collect::<Vec<_>>().join("\n\n---\n\n");
                        Ok(text)
                    }
                }
                ScratchpadAction::Modify => {
                    let id = input.id.ok_or_else(|| "Error: id required for modify.".to_string())?;
                    let text = input.content.ok_or_else(|| "Error: content required for modify.".to_string())?;
                    if let Some(_target) = entries.iter().find(|e| e.id == id) {
                        let escaped_id = escape_reg_exp(&id);
                        let pattern = format!(r"(?m)(### \[[^\]]+\] \d{{4}}-\d{{2}}-\d{{2}} \| ID: {}\n)([\s\S]*?)(\n---\n)", escaped_id);
                        let re = Regex::new(&pattern).map_err(|e| e.to_string())?;
                        
                        if !re.is_match(&block.section) {
                            return Err(format!("Error: could not locate entry {} for update.", id));
                        }
                        
                        let updated_section = re.replace(&block.section, |caps: &regex::Captures| {
                            let header = caps[1].trim_end();
                            let sep = &caps[3];
                            format!("{} [updated {}]\n{}{}", header, get_clock_time(), text.trim(), sep)
                        }).to_string();
                        
                        let _ = fs::write(&file_path, format!("{}{}{}", block.before, updated_section, block.after));
                        Ok(format!("Checkpoint {} updated.", id))
                    } else {
                        Err(format!("Note {} not found in scratchpad.", id))
                    }
                }
                ScratchpadAction::Delete => {
                    let id = input.id.ok_or_else(|| "Error: id required.".to_string())?;
                    let escaped_id = escape_reg_exp(&id);
                    let pattern = format!(r"(?m)### \[[^\]]+\] \d{{4}}-\d{{2}}-\d{{2}} \| ID: {}\n[\s\S]*?\n---\n\n?", escaped_id);
                    let re = Regex::new(&pattern).map_err(|e| e.to_string())?;
                    let updated_section = re.replace(&block.section, "").to_string();
                    let _ = fs::write(&file_path, format!("{}{}{}", block.before, updated_section, block.after));
                    Ok(format!("Deleted note {}.", id))
                }
                ScratchpadAction::Clear => {
                    let content = format!("# 📝 Scratch Pad — Temporary Notes\n\n{}\n\n---\n\n", WORKING_NOTES_SECTION);
                    let _ = fs::write(&file_path, content);
                    Ok("Scratchpad cleared.".to_string())
                }
            }
        })
    }
}
