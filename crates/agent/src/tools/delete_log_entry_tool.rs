use crate::{AgentTool, ToolCallEventStream, ToolInput};
use agent_client_protocol::schema as acp;
use gpui::{App, SharedString, Task};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteLogEntryInput {
    /// The date of the log in YYYY-MM-DD format.
    pub date: String,
    /// The ID of the log entry to delete.
    pub entry_id: String,
}

pub struct DeleteLogEntryTool;

impl AgentTool for DeleteLogEntryTool {
    type Input = DeleteLogEntryInput;
    type Output = String;

    const NAME: &'static str = "delete_log_entry";

    fn description() -> SharedString {
        "Deletes a specific log entry by date and ID. Returns whether the entry was found and removed. The date (YYYY-MM-DD) is the first 10 characters of the entry ID returned by log_task_completion.".into()
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Delete log entry".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |_cx| {
            let input = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;

            match nir_analytics::delete_log_entry(&input.date, &input.entry_id) {
                Ok(true) => Ok(format!("Deleted log entry `{}` from {}.", input.entry_id, input.date)),
                Ok(false) => Ok(format!("Log entry `{}` not found in {}.", input.entry_id, input.date)),
                Err(e) => Err(format!("Failed to delete log entry: {e}")),
            }
        })
    }
}
