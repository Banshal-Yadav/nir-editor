use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema as acp;
use chrono::Utc;
use gpui::{App, Entity, SharedString, Task};
use project::Project;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{AgentTool, ToolCallEventStream, ToolInput};

// ===== Shared types & helpers =====

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<String>,
}

pub fn project_memory_path(project: &Entity<Project>, cx: &App) -> Option<PathBuf> {
    project
        .read(cx)
        .visible_worktrees(cx)
        .next()
        .map(|wt| wt.read(cx).abs_path().join(".void").join("memory.jsonl"))
}

pub fn global_memory_path() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string())
    } else {
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    };
    PathBuf::from(base).join(".void").join("memory.jsonl")
}

pub fn read_memories(path: &PathBuf) -> Vec<MemoryEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut deleted_ids = std::collections::HashSet::new();
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<MemoryEntry>(line) {
            if let Some(ref del_id) = entry.deleted {
                deleted_ids.insert(del_id.clone());
            } else {
                entries.push(entry);
            }
        }
    }

    entries.retain(|e| !deleted_ids.contains(&e.id));
    entries
}

fn append_entry(path: &PathBuf, entry: &MemoryEntry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {e}"))?;
    }

    let json =
        serde_json::to_string(entry).map_err(|e| format!("Failed to serialize: {e}"))?;

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open memory file: {e}"))?;

    writeln!(file, "{}", json).map_err(|e| format!("Failed to write: {e}"))?;

    Ok(())
}

// ===== SAVE MEMORY TOOL =====

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
#[schemars(inline)]
pub enum MemoryScope {
    /// Store globally, accessible across all projects (personal info, preferences).
    Global,
    /// Store for current project only (architecture, conventions, patterns).
    #[default]
    Project,
}

/// Save important information to persistent memory for future conversations.
/// Use this when:
/// - The user states a preference ("I prefer tabs", "always use TypeScript")
/// - You discover a key project pattern or architecture detail
/// - The user says "remember this" or "don't forget"
/// - The user corrects you about something project-specific
/// - The user shares personal info (name, role, timezone, company)
/// Memories persist across conversations and help you provide better assistance.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SaveMemoryInput {
    /// The information to remember. Be concise but complete.
    content: String,
    /// Where to store: "global" for cross-project info, "project" for current project only.
    #[serde(default)]
    scope: MemoryScope,
}

pub struct SaveMemoryTool {
    project: Entity<Project>,
}

impl SaveMemoryTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { project }
    }
}

impl AgentTool for SaveMemoryTool {
    type Input = SaveMemoryInput;
    type Output = String;

    const NAME: &'static str = "save_memory";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Saving to memory".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        let project_path = project_memory_path(&self.project, cx);

        cx.spawn(async move |_cx| {
            let input = input
                .recv()
                .await
                .map_err(|e| format!("Failed to receive input: {e}"))?;

            let memory_path = match input.scope {
                MemoryScope::Global => global_memory_path(),
                MemoryScope::Project => {
                    project_path.ok_or_else(|| "No project root found".to_string())?
                }
            };

            let id = format!("mem_{}", Utc::now().timestamp_millis());
            let entry = MemoryEntry {
                id: id.clone(),
                content: input.content.clone(),
                created_at: Utc::now().to_rfc3339(),
                deleted: None,
            };

            append_entry(&memory_path, &entry)?;

            let scope_label = match input.scope {
                MemoryScope::Global => "global",
                MemoryScope::Project => "project",
            };

            Ok(format!(
                "Memory saved ({scope_label}, id: {id}): {}",
                input.content
            ))
        })
    }
}

// ===== RECALL MEMORY TOOL =====

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
#[schemars(inline)]
pub enum RecallScope {
    /// Search only global memories.
    Global,
    /// Search only project memories.
    Project,
    /// Search both global and project memories.
    #[default]
    All,
}

/// Search your saved memories about this project or general preferences.
/// Use this when:
/// - Starting work on a task to check for saved preferences or patterns
/// - The user asks "do you remember..." or references past context
/// - You need to verify a project convention before making changes
/// - You want to check the user's personal preferences or info
/// Pass an empty query to list all memories, or a search term to filter.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecallMemoryInput {
    /// Search query to filter memories. Leave empty to list all.
    #[serde(default)]
    query: String,
    /// Which memories to search: "global", "project", or "all".
    #[serde(default)]
    scope: RecallScope,
}

pub struct RecallMemoryTool {
    project: Entity<Project>,
}

impl RecallMemoryTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { project }
    }
}

impl AgentTool for RecallMemoryTool {
    type Input = RecallMemoryInput;
    type Output = String;

    const NAME: &'static str = "recall_memory";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Searching memory".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        let project_path = project_memory_path(&self.project, cx);

        cx.spawn(async move |_cx| {
            let input = input
                .recv()
                .await
                .map_err(|e| format!("Failed to receive input: {e}"))?;

            let mut all_memories: Vec<(String, MemoryEntry)> = Vec::new();

            let search_global = matches!(input.scope, RecallScope::Global | RecallScope::All);
            let search_project = matches!(input.scope, RecallScope::Project | RecallScope::All);

            if search_global {
                let path = global_memory_path();
                for entry in read_memories(&path) {
                    all_memories.push(("global".to_string(), entry));
                }
            }

            if search_project {
                if let Some(ref path) = project_path {
                    for entry in read_memories(path) {
                        all_memories.push(("project".to_string(), entry));
                    }
                }
            }

            // Filter by query (case-insensitive substring match)
            let query = input.query.trim().to_lowercase();
            if !query.is_empty() {
                all_memories.retain(|(_, entry)| entry.content.to_lowercase().contains(&query));
            }

            if all_memories.is_empty() {
                return if query.is_empty() {
                    Ok("No memories saved yet.".to_string())
                } else {
                    Ok(format!("No memories found matching \"{query}\"."))
                };
            }

            let mut output = format!("Found {} memories:\n\n", all_memories.len());
            for (scope, entry) in &all_memories {
                output.push_str(&format!(
                    "- [{}] (id: {}, saved: {}) {}\n",
                    scope, entry.id, entry.created_at, entry.content
                ));
            }

            Ok(output)
        })
    }
}
