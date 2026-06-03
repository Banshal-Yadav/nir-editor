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
    let database_path = nir_analytics::get_state_db_path()
        .context("Failed to resolve state database path")?;

    if input.last_session_fallback.unwrap_or(false) {
        let records = nir_analytics::recent_checkpoints(&database_path)
            .context("Failed to load recent checkpoints from state database")?;

        if records.is_empty() {
            return Ok("No historical logs found. Memory is currently empty.".to_string());
        }
        return Ok(records
            .iter()
            .map(format_record_line)
            .collect::<Vec<_>>()
            .join("\n"));
    }

    let query = input.query.unwrap_or_default();
    if query.trim().is_empty() {
        return Ok("Please provide a search query or enable last session fallback.".to_string());
    }

    let records = nir_analytics::search_checkpoints_by_text(&database_path, &query)
        .context("Failed to execute FTS5 search against state database")?;

    if records.is_empty() {
        return Ok(format!("No historical log matches found for query: '{}'", query));
    }

    Ok(records
        .iter()
        .map(format_record_line)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Formats a checkpoint record as `[YYYY-MM-DD] <summary>` for human-readable recall output.
fn format_record_line(record: &nir_analytics::CheckPointRecord) -> String {
    let date = record
        .id
        .get(..10)
        .unwrap_or("Unknown Date");
    format!("[{}] {}", date, record.summary)
}
