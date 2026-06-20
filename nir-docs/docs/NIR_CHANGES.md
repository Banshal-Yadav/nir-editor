# /nir Fork — Detailed Change Analysis

This report documents the changes made to the /nir fork of Zed, including successful implementations and attempted features.

## [v1.6.0-nir] - 2026-05-30
### Merged from upstream
- Zed v1.6.0 (e2e7a6769e) — 187 commits, 597 files
- +74,343 / -18,487 lines

### New features landed
- Claude Opus 4.8 support
- Mermaid/Merman rendering engine
- GPUI Accesskit (0.24.0 / Windows 0.32.1 / macOS 0.26.0 / Unix 0.21.0)
- Terminal sandboxing (macOS Seatbelt)
- Skill system: import from URL, sharing links, dedup
- AGENTS.md editing (global + project rules)
- LSP document links support
- Handoff feature flag
- Notebook cell improvements
- DevContainer + rustup rust-analyzer toolchain
- New git forge icons (Bitbucket, Codeberg, Forgejo, Gitea)
- Vim fixes (dot repeat, helix select, n/N, replace)
- Git panel fixes (file diffs, commit prompt, worktree)
- GPL-3.0-or-later license headers added

### /nir features retained
- Agent Launcher + AgentLauncherButton
- Scratchpad + Brain Memory tools
- Custom system prompts
- Diff colors + thread view borders
- /nir icons, themes, amber brutalist UI
- upgrade_prompt.rs (Razorpay TODO)
- NIR_EXPERIMENTAL_A11Y

### Removed
- All 51 Zed GitHub Actions workflows
- Zed PR/issue templates and CODEOWNERS

---

## 🟢 Context Token Tracking for Local Models (2026-06-20)

### 1. LM Studio Telemetry Support
- **Issue**: The agent panel's Context Ring UI did not populate token usage for local models running via LM Studio because LM Studio streams usage differently than OpenAI.
- **Fixes**:
    - Appended `stream_options: { include_usage: true }` to the `ChatCompletionRequest` in `lmstudio.rs`.
    - Rewrote the `map_event` SSE parser in `lmstudio.rs` to gracefully handle the final chunk where the `choices` array is empty but the `usage` object is populated, preventing stream abort/disconnects.
    - Adjusted `latest_token_usage` extraction in `thread_view.rs` to render the context ring UI block immediately upon model selection.

### 2. Path Resolution Diagnostics (Root Cause Identified)
- **Issue**: Gemma 12B failed to use the `list_directory` tool correctly, outputting unparseable paths like `.`` and throwing "Path is not in project" errors.
- **Root Cause Discovered**: We identified a direct contradiction in the codebase's prompt engineering:
    - `system_prompt.hbs` explicitly tells the model: *"ALWAYS use the full absolute path... NEVER use '.'"*.
    - The JSON schemas for `list_directory`, `read_file`, and `write_file` tools strictly tell the model: *"This path should never be absolute"*.
    - This contradiction caused the 12B model to hallucinate its pre-trained baseline (`.`) but with a syntactical JSON error (`.``), bypassing Zed's fallback handler.

---

### 1. Rebranding & UI Identity
- **Global Rebranding**: Replaced "Zed" with "/nir" across the entire codebase, including UI strings, documentation, and metadata.
- **Custom Icons**: 
    - Updated `assets/icons/file_icons/ai.svg` and `assets/icons/ai_NIR.svg` with the new /nir monospace aesthetic.
    - Updated `IconName::ZedAgent` and `IconName::ZedAssistant` to `IconName::NIRAgent` and `IconName::NIRAssistant`.
- **Brutalist Theme**: Implemented "NIR Dark" brutalist theme with minimal status bar.
- **Antigravity Theme Fix**: Corrected the Stop generating button's background color to ensure the stop icon remains visible during active generation.

### 4. Better Brutalist UI (New)
- **Welcome Screen**: Redesigned as an industrial, high-impact welcome page. Features thick 3px borders, a massive brand mark box, and a solid 20px offset shadow for a physical presence.
- **Industrial Grid**: Main navigation is now a high-contrast 2x2 grid with inverted hover states (Amber background / Black text).
- **AI Setup Redesign**: Refined the AI onboarding with subtle amber tints for featured cards and standardized pill-shaped buttons.
- **Unified Brand Mark**: Replaced all legacy agent icons with the standardized `[/]` brand mark across the activity bar, agent panel header, and welcome screen.
- **Agent Panel Dock Icon**: Changed the bottom-bar toggle icon for the Agent Panel from `[/]` to a `Sparkle` icon. Removed the "Agent" text label to keep the status bar minimalist, and implemented dynamic color highlighting (Accent color) when the panel is active.
- **Activity Bar Migration**: Successfully migrated the "Open Threads Sidebar" toggle from the sidebar's bottom bar to the new Activity Bar (positioned directly beneath the logo).
- **Logo Spacing**: Adjusted the Activity Bar logo spacing by increasing the bottom padding/margin to match the overall layout hierarchy.
- **Dynamic State Icons**: The sidebar toggle icon in the Activity Bar now dynamically updates its visual state (`Open` vs `Closed`) based on the sidebar's current visibility.
- **Contextual Visibility**: The Agent toggle is dynamically hidden if Agent settings are disabled or the button preference is turned off.
- **Visual Hierarchy**: Added a divider between the navigation controls and the project list for better visual separation in the Activity Bar.
- **Status Bar Purge**: Removed all redundant sidebar toggles from the status bar, enforcing a minimalist brutalist layout centered on essential information (branch, language, position).
- **Native Agent Branding**: Standardized the Native Agent server's branding to the `IconName::NIRAgent` (`[/]`) icon.
- **Sidebar Clean-up**: Relocated the "Show Thread History" clock icon to the left of the sidebar's bottom bar and removed the old toggle button to reduce UI clutter.
- **Terminal Integration**: Configured `TERM_PROGRAM` env var to "NIR" and `ZED_TERM` to `NIR_TERM` to ensure correct host identification in terminal sessions.
- **Copilot UI Rebranding**: Updated text strings in the Copilot sign-in flow and agent usage modals to correctly reference the `/nir` brand.
- **Agent New Chat Layout**: Centered the input prompt for empty chats, reduced its height to 10vh, and introduced a subtle "What do you want to build?" greeting to enhance the brutalist idle state.
- **Sidebar Thread Filtering**: Successfully refactored `Sidebar::rebuild_contents` to filter the agent thread list based on the active workspace selected in the Activity Bar. This ensures that only relevant conversations are visible, reducing cognitive load when working on multiple projects.
- **Sidebar Agent Toggle**: Added a secondary Agent Panel toggle button (Sparkle icon) to the Activity Bar, positioned above the sidebar toggle. This provides a high-level navigation entry point that supports both opening/focusing and closing the panel with a single click.
- **Activity Bar Divider**: Added a visual divider between the top-level navigation buttons and the workspace project list to improve layout hierarchy.

### 5. AI Chat Experience (New)
- **Greeter UI Refinement**: Centered the "What do you want to build today?" welcome text and the `[/]` brand mark, creating a clean symmetry.
- **Improved Metadata Display**: Added a persistent metadata bar in the message editor showing the current file, its extension, and line range, providing better context for AI interactions.
- **GPUI Implementation Fixes**: 
    - Resolved `Div` vs `Stateful<Div>` type errors by correctly wrapping lists in `div()` before applying `vertical_scrollbar_for`.
    - Sanitized `message_editor.rs` visibility issues and removed redundant/unused imports in `sidebar.rs`.

---

## 🛠️ Planned Improvements (In Progress)

### 1. Suggestion Chips (Re-implementation)
- **Objective**: Restore the dynamic action bar above the message editor with improved stability and contextual accuracy.
- **Planned Features**:
    - **Git-Aware Suggestion**: "Summarize Changes" pill based on `GitStore`.
    - **Language-Aware Hints**: "Improve UI", "Refactor", and "Explain" chips.
    - **One-Click Execution**: Functional integration for instant prompt execution.

---

## 🟢 User-Facing String Rebranding (2026-05-09)

### 1. Extensions UI
- Updated 18 feature banners: "/nir comes with basic Git support...", "/nir supports linking to a source line..."
- All built-in feature labels: "Shell/C/C++/Go/Python/React/Rust/Typescript support is built-in to /nir!"

### 2. AI Onboarding
- Plan token descriptions: "$X of tokens in /nir agent" (3 occurrences)

### 3. Language Model Providers
- Updated 21 API key setup messages across 9 provider files:
  - OpenAI, xAI, Vercel AI Gateway, Vercel v0, OpenCode, OpenAI-Compatible, Ollama, Mistral, LM Studio
- Error messages and status labels

### 4. Other Components
- Debugger: "/nir cannot determine how to run this debug scenario..."
- Edit Predictions: "/nir's Edit Predictions"
- CLI output: "/nir {version} – {path}"
- Copilot Chat: "/nir/{}" user-agent header

### Status: ✅ Complete
Build verified successfully. All user-facing UI strings rebranded.

---

## 🟢 Agent Edit Tool Modernization (2026-05-10)

### PR #55612 — Remove old edit file tool

Applied upstream Zed changes to modernize the agent's edit tool:

- **Deleted**: `crates/agent/src/tools/edit_file_tool.rs` (old non-streaming tool)
- **Updated** `crates/agent/src/tools.rs`:
  - Removed `mod edit_file_tool;`
  - Removed `pub use edit_file_tool::*;`
  - Replaced `EditFileTool` with `StreamingEditFileTool` in the tools macro
- **Updated** `crates/agent/src/tools/streaming_edit_file_tool.rs`:
  - Changed `const NAME` from `"streaming_edit_file"` to `"edit_file"`
  - Removed broken import `use super::edit_file_tool::EditFileTool;`
- **Updated** `crates/agent/src/thread.rs`:
  - Replaced all `EditFileTool` references with `StreamingEditFileTool`
  - Removed duplicate tool registration
  - Simplified profile name logic (uses string "edit_file" instead of struct)

### Rationale
- The old edit tool required two LLM requests per edit ( wasteful)
- The streaming tool supports single-request editing with real-time diff preview
- All agent tools now use the unified "edit_file" name for user configuration

### Status: ✅ Complete

---

## 🟢 Upstream Zed PR Integration (2026-05-10)

### Agent PRs Applied
- **#55612** — Removed old edit_file_tool.rs, kept streaming_edit_file as primary "edit_file" tool
- **#55606** — Added file_changed_since_last_read tracking, improved error messages for modified files
- **#55193** — Removed StreamingEditFileToolFeatureFlag, streaming edit now always enabled

### Other PRs Verified (Already Present)
- **#55946/55947** — ACP non_interactive shell (acp.rs:696)
- **#55942** — inotify Access event check (fs_watcher.rs:475)
- **#55500** — mode parameter in StreamingEditFileToolInput
- **#55765** — ACP error handling
- **#55775** — npm_command with prefix_dir support
- **#56026** — Added reload() to MarkdownPreviewView (APPLIED THIS SESSION)
- **#54570** — follow_tail re-engaging after scrollbar
- **#53685** — Separate AgentUiFontSize and AgentBufferFontSize
- **#54683** — Permission buttons pass correct tool_call_id

### Status: ✅ All Verified/Applied

---

## 🟢 Chat Input Enhancements (2026-05-09)

### 1. Layout Improvements
- Increased input box padding: `.px_2()` → `.px_4()`
- Made greeting responsive: text wraps on narrow screens
- Reduced text size: 28px → 24px for better fit

### 2. Pulsing Animation
- Added 2-second looping pulse to sparkle icon
- Uses `pulsating_between(0.5, 1.0)` easing
- Working implementation in `thread_view.rs`

### Documentation
- Created `NIR-docs/animation.md` with complete animation guide
- Includes: API reference, easing functions, common pitfalls, working patterns

### Status: ✅ Complete

---


---

## 📄 Useful Code Snippets

### Activity Bar Implementation (`multi_workspace.rs`)
```rust
fn render_activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let active_workspace = self.workspace();
    // ... hashing logic for colors ...
    div()
        .id(("workspace_avatar", workspace_id))
        .when(is_active, |el| {
            el.child(div().absolute().left_0().w(px(2.)).bg(gpui::rgb(0xffb000)))
        })
        .child(div().bg(avatar_color).child(first_letter.to_string()))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.activate(workspace_clone.clone(), None, window, cx);
        }))
}
```

### Thread Store Helper (`thread_metadata_store.rs`)
```rust
pub fn entries_for_workspace<'a>(
    &'a self,
    path_list: &'a PathList,
) -> impl Iterator<Item = &'a ThreadMetadata> + 'a {
    self.entries_for_path(path_list, None)
}
```
