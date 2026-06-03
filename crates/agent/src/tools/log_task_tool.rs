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
        "Call this tool ONLY when a user-requested task, technical implementation, or milestone is fully finished. Provide a clear, concise summary of the engineering work done.".into()
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Logging task completion".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |_cx| {
            let input = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;
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
                id: entry_id,
                timestamp: chrono::Utc::now().timestamp(),
                category: "task_completion".to_string(),
                summary: input.task_completed,
                tags: String::new(),
                error_recovery: false,
            };
            nir_analytics::insert_checkpoint(&connection, &checkpoint)
                .context("Failed to write checkpoint to FTS5 index")
                .map_err(|e| e.to_string())?;

            Ok("Milestone logged successfully.".to_string())
        })
    }
}
