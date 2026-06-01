use crate::{AgentTool, ToolCallEventStream, ToolInput};
use agent_client_protocol::schema as acp;
use gpui::{App, SharedString, Task};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
            match nir_analytics::write_daily_log(&log_line) {
                Ok(_) => Ok("Milestone logged successfully.".to_string()),
                Err(e) => Err(e.to_string()),
            }
        })
    }
}
