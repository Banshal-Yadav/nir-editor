# /void Rebrand â€” Progress Report

> Generated: 2026-04-28
> Based on: void-docs audit + all changes applied this session

---

## Current Status Overview
- Build fix (copilot_ui): Fixed
- App identity (display names): Fixed
- Filesystem paths (all platforms): Fixed
- Package metadata: Fixed
- Welcome screen /void rebranding: Complete
- Brutalist UI Overhaul: Complete
- Logo / icon asset: Standardized [/] mark implemented
- Menu items & help links: Ongoing
- zed.dev URL replacement: Placeholder phase

---

## âœ… Done This Session

### 1. release_channel/lib.rs (Rebranding)
- UI Title bar: "Void" / "Void Dev" / "Void Nightly"
- App ID: "dev.void.Void"
- Windows App ID: "Void-Editor-Dev"
- Env Vars: VOID_RELEASE_CHANNEL, VOID_APP_VERSION

### 2. paths/paths.rs (Data Dirs)
- macOS App Support: Library/Application Support/Void
- macOS Logs: Library/Logs/Void
- Project folder: .void/
- Settings: .void/settings.json
- Tasks/Debug: .void/tasks.json, .void/debug.json

Linux/Windows paths were already using `void` from a previous session.

### 3. zed/Cargo.toml (Metadata)
- Package Name: void
- Version: 0.1.0
- Authors: Void Team
- Binary: void
- Bundle IDs: dev.void.Void*
- URL Schemes: void://

### 4. Root `Cargo.toml` â€” Build Fix

`copilot_ui` crate existed but was never registered in the workspace.
All 4 dependent crates (`zed`, `settings_ui`, `language_models`, `edit_prediction`, `edit_prediction_ui`)
referenced it as `copilot_ui.workspace = true` which caused build failure.

**Fixed:**
- Added `"crates/copilot_ui"` to `[workspace.members]`
- Added `copilot_ui = { path = "crates/copilot_ui" }` to `[workspace.dependencies]`

---

## âœ… Done Before This Session (Verified in void-docs)

### 4. Verified Before This Session
- Windows Class Names: Void::Window
- Windows Platform: Void::PlatformWindow
- Agent ID: VOID_AGENT_ID
- Telemetry: Disabled via default settings
- Welcome Screen: /void Branding
- Linux/Windows Dirs: void-prefixed

---

## ðŸ”´ What Still Needs Doing

### HIGH â€” User-visible, must fix before ship

### High Priority Remaining
- Menu "About Void": help links and about section
- Onboarding Strings: "Welcome to /void"
- Settings UI: Update remaining "Zed" descriptions
- Troubleshooting URLs: Update to void placeholders

### MEDIUM â€” Runtime/functional impact

### Medium Priority Remaining
- Cloud API URLs: Transition away from api.zed.dev
- Provider ID: Update ZED_CLOUD_PROVIDER_ID to void.dev
- DB Migration: Update provider string in migrator
- OpenRouter Headers: Update Referer to voideditor.com
- Feedback Email: hi@zed.dev -> void-feedback@placeholder.com

### LOW â€” Cosmetic / can wait

### Low Priority Remaining
- 130+ zed.dev URL refs: Update to void placeholders
- .zed_server remote path: Change to .void_server
- Git default email: hi@zed.dev -> void-feedback@placeholder.com
- UI website buttons: zed.dev labels -> voideditor.com

---

## Agent UI Redesign (Planned, Not Started)

Documented in `void-agent-layout.md`. Key planned changes:

- Remove always-rendering onboarding banner
- Remove trial upsell banner
- Simplify toolbar to single row
- Integrate auth state into chat view (no separate auth screen)
- Add context bar at bottom (Files / Memory / Tools tabs)

Files to touch when ready: `agent_panel.rs`, `conversation_view.rs`, `title_bar.rs`

---

*Report generated: 2026-04-28*
*Session: void rebrand + build fix*

---

## Sidebar Implementation Attempt — 2026-05-01

### Summary
Attempted to filter the agent thread sidebar to only show threads relevant to the active workspace. This is a key part of the /void UI simplification strategy.

### Changes Made
- **Thread Store**: Added entries_for_workspace to ThreadMetadataStore to facilitate workspace-based filtering.
- **Activity Bar**: Successfully implemented the narrow left activity bar (multi_workspace.rs) which allows switching between workspaces.
- **Rebranding**: Continued rebranding efforts in sidebar.rs, workspace.rs, and pane.rs.
- **Icons**: Redesigned i.svg and i_void.svg to match the new /void monospace aesthetic.

### Status: ?? Broken
The sidebar filtering logic in sidebar.rs is not yet functional. The backend method exists, but ebuild_contents still displays all project groups and threads. Attempting to force the filter within the existing loop structure proved complex and requires a more surgical refactor of the sidebar's rendering pipeline.

### Blocks / Issues
- Sidebar::rebuild_contents is tightly coupled to the "All Workspaces" view.
- Lifetime and iterator issues when trying to integrate entries_for_workspace into the main list generation.

### Next Session Goals
- Refactor Sidebar::rebuild_contents to optionally accept a workspace filter.
- Ensure the sidebar accurately reflects the project selected in the new Activity Bar.


---

## ✅ Phase 2 Rebranding — 2026-05-01

Finalized the UI rebranding by replacing remaining Zed strings and URLs with Void identifiers or placeholders.

### Placeholder URL Documentation
The following placeholders were introduced to replace `zed.dev` links that are not yet live for Void:

- **General Docs / Troubleshooting**: `https://void.placeholder/coming-soon`
- **Feedback Email**: `void-feedback@placeholder.com` (replaced `hi@zed.dev`)
- **Redirects**: Existing `voideditor.com` links were maintained where functional.

### Files Rebranded
- `crates/zed/src/zed/app_menus.rs`: Updated all menu display strings to "/void".
- `crates/onboarding/src/onboarding.rs`: Rebranded documentation constants and comments.
- `crates/onboarding/src/multibuffer_hint.rs`: Rebranded multibuffer help link.
- `crates/settings_ui/src/page_data.rs`: Verified/Updated remaining Zed descriptions to /void.
- `crates/zed/src/zed.rs`: Fixed a hardcoded "Zed" string in the GPU warning prompt.
- `crates/feedback/src/feedback.rs`: Rebranded feedback email to placeholder.
