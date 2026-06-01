use anyhow::{Result, Context};
use chrono::Local;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use gpui::{App, SharedString, Task};
use agent_client_protocol::schema as acp;
use crate::{AgentTool, ToolCallEventStream, ToolInput};

#[derive(Deserialize, Serialize, JsonSchema, Clone)]
pub struct RecordCheckpointArgs {
    /// A dense, 1-2 sentence summary of the exact technical milestone achieved.
    pub summary: String,
    
    /// The technical domain of the work (e.g., "Refactoring", "Feature", "Bugfix", "UI")
    pub category: String,

    /// Optional metrics for high-signal friction tracking (e.g., ["error_recovery"], ["user_intervention"])
    pub context_tags: Option<Vec<String>>,
}

pub struct RecordCheckpointTool;

impl RecordCheckpointTool {
    pub async fn execute(&self, args: RecordCheckpointArgs) -> Result<String> {
        let home_dir = dirs::home_dir().context("Failed to resolve home directory")?;
        let log_dir = home_dir.join(".nir/brain/logs");
        std::fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

        let current_date = Local::now().format("%Y-%m-%d").to_string();
        let log_file_path = log_dir.join(format!("{}.md", current_date));

        let current_time = Local::now().format("%H:%M:%S").to_string();
        let tags = args.context_tags.unwrap_or_default();
        let tags_str = if tags.is_empty() {
            String::new()
        } else {
            format!(" Tags: [{}]", tags.join(", "))
        };

        let log_line = format!(
            "[{}] [{}] {}{}\n",
            current_time,
            args.category.to_uppercase().trim(),
            args.summary.trim(),
            tags_str
        );

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .context("Failed to open daily log file")?;

        file.write_all(log_line.as_bytes())
            .context("Failed to write checkpoint to log file")?;

        Ok("Checkpoint recorded successfully.".to_string())
    }
}

impl AgentTool for RecordCheckpointTool {
    type Input = RecordCheckpointArgs;
    type Output = String;

    const NAME: &'static str = "record_checkpoint";

    fn description() -> SharedString {
        "Record a checkpoint when completing a meaningful logical step, bugfix, refactor, or architectural choice.".into()
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Recording checkpoint".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |_cx| {
            let args = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;
            self.execute(args).await.map_err(|e| e.to_string())
        })
    }
}
