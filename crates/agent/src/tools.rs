mod apply_code_action_tool;
mod context_server_registry;
mod copy_path_tool;
mod create_directory_tool;
mod create_thread_tool;
mod delete_log_entry_tool;
mod delete_path_tool;
mod diagnostics_tool;
mod edit_file_tool;
mod edit_session;
#[cfg(all(test, feature = "unit-eval"))]
mod evals;
mod fetch_tool;
mod find_path_tool;
mod find_references_tool;
mod get_code_actions_tool;
mod go_to_definition_tool;
mod grep_tool;
mod list_agents_and_models_tool;
mod list_directory_tool;
mod log_task_tool;
mod recall_past_context_tool;
mod brain_memory_tool;
mod backup_tool;
mod scratchpad_tool;
mod move_path_tool;
mod read_file_tool;
mod rename_tool;
mod skill_tool;
mod spawn_agent_tool;
mod symbol_locator;
mod terminal_tool;
mod tool_permissions;
mod update_plan_tool;
mod update_title_tool;
mod web_search_tool;
mod write_file_tool;

use crate::AgentTool;
use gpui::App;
use language_model::{LanguageModelRequestTool, LanguageModelToolSchemaFormat};
use serde::{
    Deserialize, Deserializer,
    de::{DeserializeOwned, Error as _},
};

/// Deserialize a value that may have been provided as a JSON-encoded string
/// instead of the structured value. Some models occasionally stringify nested
/// arguments, so we accept either form.
pub(crate) fn deserialize_maybe_stringified<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: DeserializeOwned,
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ValueOrJsonString<T> {
        Value(T),
        String(String),
    }

    match ValueOrJsonString::<T>::deserialize(deserializer)? {
        ValueOrJsonString::Value(value) => Ok(value),
        ValueOrJsonString::String(string) => serde_json::from_str::<T>(&string).map_err(|error| {
            D::Error::custom(format!("failed to parse stringified value: {error}"))
        }),
    }
}

pub use apply_code_action_tool::*;
pub use context_server_registry::*;
pub use copy_path_tool::*;
pub use create_directory_tool::*;
pub use create_thread_tool::*;
pub use delete_log_entry_tool::*;
pub use delete_path_tool::*;
pub use diagnostics_tool::*;
pub use edit_file_tool::*;

pub use fetch_tool::*;
pub use find_path_tool::*;
pub use find_references_tool::*;
pub use get_code_actions_tool::*;
pub use go_to_definition_tool::*;
pub use grep_tool::*;
pub use list_agents_and_models_tool::*;
pub use list_directory_tool::*;
pub use log_task_tool::*;
pub use brain_memory_tool::*;
pub use backup_tool::*;
pub use scratchpad_tool::*;
pub use move_path_tool::*;
pub use recall_past_context_tool::*;
pub use read_file_tool::*;
pub use rename_tool::*;
pub use skill_tool::*;
pub use spawn_agent_tool::*;
pub use symbol_locator::*;

pub use terminal_tool::*;
pub use tool_permissions::*;
pub use update_plan_tool::*;
pub use update_title_tool::*;
pub use web_search_tool::*;
pub use write_file_tool::*;

macro_rules! tools {
    ($($tool:ty),* $(,)?) => {
        /// Every built-in tool name, determined at compile time.
        pub const ALL_TOOL_NAMES: &[&str] = &[
            $(<$tool>::NAME,)*
        ];

        const _: () = {
            const fn str_eq(a: &str, b: &str) -> bool {
                let a = a.as_bytes();
                let b = b.as_bytes();
                if a.len() != b.len() {
                    return false;
                }
                let mut i = 0;
                while i < a.len() {
                    if a[i] != b[i] {
                        return false;
                    }
                    i += 1;
                }
                true
            }

            const NAMES: &[&str] = ALL_TOOL_NAMES;
            let mut i = 0;
            while i < NAMES.len() {
                let mut j = i + 1;
                while j < NAMES.len() {
                    if str_eq(NAMES[i], NAMES[j]) {
                        panic!("Duplicate tool name in tools! macro");
                    }
                    j += 1;
                }
                i += 1;
            }
        };

        /// Returns whether the tool with the given name supports the given provider.
        pub fn tool_supports_provider(name: &str, provider: &language_model::LanguageModelProviderId) -> bool {
            $(
                if name == <$tool>::NAME {
                    return <$tool>::supports_provider(provider);
                }
            )*
            false
        }

        /// Returns whether the tool with the given name may be provided to an
        /// agent in a restricted workspace. Unknown tools (e.g. MCP tools) are
        /// considered allowed.
        pub fn tool_allowed_in_restricted_mode(name: &str) -> bool {
            $(
                if name == <$tool>::NAME {
                    return <$tool>::allow_in_restricted_mode();
                }
            )*
            true
        }

        /// A list of all built-in tools
        pub fn built_in_tools() -> impl Iterator<Item = LanguageModelRequestTool> {
            fn language_model_tool<T: AgentTool>() -> LanguageModelRequestTool {
                LanguageModelRequestTool::function(
                    T::NAME.to_string(),
                    T::description().to_string(),
                    T::input_schema(LanguageModelToolSchemaFormat::JsonSchema).to_value(),
                    T::supports_input_streaming(),
                )
            }
            [
                $(
                    language_model_tool::<$tool>(),
                )*
            ]
            .into_iter()
        }
    };
}

// Adding a tool here (and constructing it in `Thread::add_default_tools`) is
// not enough to make the model actually receive it. Three further gates will
// silently drop the tool rather than fail to compile:
//
// 1. `assets/settings/default.json`: the `write` and `ask` agent profiles each
//    carry an explicit `tools` allowlist. `Thread::enabled_tools` filters out
//    any tool not present there with value `true`, so it never reaches the
//    model.
// 2. `test_all_tools_are_in_tool_info_or_excluded` in
//    `crates/settings_ui/src/pages/tool_permissions_setup.rs`: every tool must
//    be in the permission-UI `TOOLS` list (if it calls
//    `decide_permission_from_settings`) or in `EXCLUDED_TOOLS`.
// 3. `tool_feature_flag_enabled`: some tools are gated behind a feature flag and
//    are dropped unless it is active. The agent-profile UI uses the same gate so
//    it never offers a tool the agent can't actually use.
tools! {
    ApplyCodeActionTool,
    CopyPathTool,
    CreateDirectoryTool,
    CreateThreadTool,
    DeleteLogEntryTool,
    DeletePathTool,
    DiagnosticsTool,
    FetchTool,
    FindPathTool,
    FindReferencesTool,
    GetCodeActionsTool,
    GoToDefinitionTool,
    GrepTool,
    ListAgentsAndModelsTool,
    ListDirectoryTool,
    MovePathTool,
    ReadFileTool,
    RenameTool,
    BrainMemoryTool,
    BackupTool,
    ScratchpadTool,
    SkillTool,
    SpawnAgentTool,
    EditFileTool,
    TerminalTool,
    WebSearchTool,
    WriteFileTool,
    RecallPastContextTool,
    LogTaskTool,
    UpdatePlanTool,
    UpdateTitleTool,
}

/// Some built-in tools are gated behind a feature flag and only become usable
/// once that flag is active. Tools without a flag are always available.
///
/// This is the single source of truth for that gating: `Thread::enabled_tools`
/// uses it to decide what the model receives, and the agent-profile
/// configuration UI uses it to decide what to offer — so the UI can never list
/// a tool the agent would silently drop (see #56778).
pub fn tool_feature_flag_enabled(_tool_name: &str, _cx: &App) -> bool {
    // All tools enabled — feature flags removed for local/offline builds.
    // Flags gated LSP tools (find_references, go_to_definition, code_actions,
    // rename_symbol) and sub-agent tools (create_thread, list_agents_and_models),
    // all of which are safe with a working LSP server.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_and_terminal_are_forbidden_in_restricted_mode() {
        assert!(!tool_allowed_in_restricted_mode(FetchTool::NAME));
        assert!(!tool_allowed_in_restricted_mode(TerminalTool::NAME));

        // Every other built-in tool, and unknown (e.g. MCP) tools, are allowed.
        for name in ALL_TOOL_NAMES {
            let expected = *name != FetchTool::NAME && *name != TerminalTool::NAME;
            assert_eq!(
                tool_allowed_in_restricted_mode(name),
                expected,
                "unexpected restricted-mode policy for tool `{name}`"
            );
        }
        assert!(tool_allowed_in_restricted_mode("some_mcp_tool"));
    }
}
