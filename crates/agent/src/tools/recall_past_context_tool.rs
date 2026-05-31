use std::fs;
use std::sync::Arc;
use anyhow::{Context, Result};
use gpui::{App, SharedString, Task};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use agent_client_protocol::schema as acp;
use crate::{AgentTool, ToolCallEventStream, ToolInput};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecallPastContextInput {
    /// Optional keywords to filter lines (e.g., 'database' or 'css'). Use when searching for specific technical topics. Do NOT include filler words like 'strategy', 'configuration', 'history', or 'logs'.
    pub query: Option<String>,
    /// Set to true when asking generic temporal questions like "What did we do last time?" or "Review my previous session" without specific keywords.
    pub last_session_fallback: Option<bool>,
}

pub struct RecallPastContextTool;

impl AgentTool for RecallPastContextTool {
    type Input = RecallPastContextInput;
    type Output = String;

    const NAME: &'static str = "recall_past_context";

    fn description() -> SharedString {
        "Search cross-session telemetry logs for historical context. Use query when searching for specific technical topics. Use last_session_fallback: true (and leave query null) when asking generic temporal questions like 'What did we do last time?' or 'Review my previous session'.".into()
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Recalling past context".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        cx.spawn(async move |_cx| {
            let input = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;
            
            execute_recall_past_context(input).await.map_err(|e| e.to_string())
        })
    }
}

async fn execute_recall_past_context(input: RecallPastContextInput) -> Result<String> {
    let home_str = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .context("Could not determine home directory from environment variables")?;
    let log_dir = std::path::PathBuf::from(home_str).join(".nir").join("brain").join("logs");
    
    if !log_dir.exists() {
        return Ok("No historical logs found. Memory is currently empty.".to_string());
    }

    if input.last_session_fallback.unwrap_or(false) {
        let mut entries = fs::read_dir(&log_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect::<Vec<_>>();
        
        entries.sort_by_key(|e| e.file_name());
        entries.reverse();

        for entry in entries {
            let path = entry.path();
            if let Ok(content) = fs::read_to_string(&path) {
                let log_lines: Vec<&str> = content.lines()
                    .filter(|l| {
                        let trimmed = l.trim();
                        !trimmed.is_empty() && !trimmed.starts_with('#')
                    })
                    .collect();

                if !log_lines.is_empty() {
                    let date_str = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unknown Date");

                    let start_idx = log_lines.len().saturating_sub(15);
                    let selected = &log_lines[start_idx..];
                    let formatted: Vec<String> = selected.iter()
                        .map(|line| format!("[{}] {}", date_str, line))
                        .collect();
                    return Ok(formatted.join("\n"));
                }
            }
        }
        return Ok("No historical logs found. Memory is currently empty.".to_string());
    }

    let query = input.query.unwrap_or_default();
    if query.trim().is_empty() {
        return Ok("Please provide a search query or enable last session fallback.".to_string());
    }

    let mut matches = Vec::new();
    let lower_query = query.to_lowercase();

    let mut entries = fs::read_dir(log_dir)?
        .filter_map(|e| e.ok())
        .collect::<Vec<_>>();
    
    // Search recent files first
    entries.sort_by_key(|e| e.file_name());
    entries.reverse();

    for entry in entries {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            let content = fs::read_to_string(&path)?;
            for line in content.lines() {
                let keywords: Vec<&str> = lower_query.split_whitespace().collect();
                if !keywords.is_empty() && keywords.iter().all(|&kw| line.to_lowercase().contains(kw)) {
                    let date_str = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unknown Date");
                        
                    let formatted_line = format!("[{}] {}", date_str, line);
                    if !matches.contains(&formatted_line) {
                        matches.push(formatted_line);
                    }
                }
                
                if matches.len() >= 40 {
                    break;
                }
            }
        }
        if matches.len() >= 40 {
            break;
        }
    }

    if matches.is_empty() {
        return Ok(format!("No historical log matches found for query: '{}'", query));
    }

    Ok(matches.join("\n"))
}
