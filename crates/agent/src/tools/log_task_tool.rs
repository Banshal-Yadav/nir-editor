use crate::{AgentTool, ToolCallEventStream, ToolInput};
use agent_client_protocol::schema as acp;
use gpui::{App, SharedString, Task};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use anyhow::Context;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LogTaskInput {
    /// A clear, concise summary of the engineering work done.
    pub task_completed: String,
}

pub struct LogTaskTool;

impl LogTaskTool {
    pub const NAME: &'static str = "log_task_completion";
}

impl AgentTool for LogTaskTool {
    type Input = LogTaskInput;
    type Output = String;

    const NAME: &'static str = "log_task_completion";

    fn description() -> SharedString {
        "Record a task completion in the persistent log. Returns an Entry ID on success (format: YYYY-MM-DD-YYYYMMDDHHMMSS-xxxx). Call this only when code was written, a bug was fixed, a file was created/modified, a build ran, a test passed, a meaningful engineering milestone was reached, or when the user explicitly asks you to log something. NEVER use this for acknowledgments, confirmations, conversation, reading files, or any action that didn't produce a concrete output.".into()
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
            return "log_task_completion".into();
        };
        let task = input.task_completed.trim();
        if task.is_empty() {
            "log_task_completion".into()
        } else if task.len() > 60 {
            format!("{}…", &task[..60]).into()
        } else {
            task.to_string().into()
        }
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |_cx| {
            let input = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;

            if !nir_analytics::load_session_config().enabled {
                return Ok("Session history disabled — task not logged.".to_string());
            }

            let log_line = format!("Completed Task: {}", input.task_completed);
            let entry_id = nir_analytics::write_daily_log(&log_line)
                .map_err(|e| e.to_string())?;

            let database_path = nir_analytics::get_state_db_path()
                .context("Failed to resolve state database path for FTS5 insert")
                .map_err(|e| e.to_string())?;
            let connection = nir_analytics::init_storage_engine(&database_path)
                .context("Failed to open state database for FTS5 insert")
                .map_err(|e| e.to_string())?;
            let checkpoint = nir_analytics::CheckPointRecord {
                id: entry_id.clone(),
                timestamp: chrono::Utc::now().timestamp(),
                category: "task_completion".to_string(),
                summary: input.task_completed,
                tags: String::new(),
                error_recovery: false,
            };
            nir_analytics::insert_checkpoint(&connection, &checkpoint)
                .context("Failed to write checkpoint to FTS5 index")
                .map_err(|e| e.to_string())?;

            Ok(format!("Milestone logged successfully. Entry ID: {entry_id}"))
        })
    }
}
