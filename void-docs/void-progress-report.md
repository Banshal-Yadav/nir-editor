# /void Rebrand â€” Progress Report

> Generated: 2026-04-28
> Based on: void-docs audit + all changes applied this session

---

## Summary

| Category | Status |
|----------|--------|
| Build fix (`copilot_ui`) | âœ… Fixed |
| App identity (display names, app IDs) | âœ… Fixed |
| Filesystem paths (all platforms) | âœ… Fixed |
| Package metadata & binary names | âœ… Fixed |
| Windows platform classes | âœ… Previously done |
| Agent ID | âœ… Previously done |
| Telemetry disabled | âœ… Previously done |
| Welcome screen strings | âœ… Previously done |
| Logo / icon asset | ðŸ”´ Not started |
| Menu items & help links | ðŸ”´ Not started |
| Settings UI strings | ðŸ”´ Not started |
| Onboarding UI strings | ðŸ”´ Not started |
| zed.dev URLs (137 refs) | ðŸ”´ Not started |
| API endpoints (cloud.zed.dev etc.) | ðŸ”´ Not started |

---

## âœ… Done This Session

### 1. `crates/release_channel/src/lib.rs`

What users see in the UI title bar and window:

| Before | After |
|--------|-------|
| `"Zed"` / `"Zed Dev"` / `"Zed Nightly"` | `"Void"` / `"Void Dev"` / `"Void Nightly"` |
| `"dev.zed.Zed"` (app ID / Wayland class) | `"dev.void.Void"` |
| `"Zed-Editor-Dev"` (Windows app ID) | `"Void-Editor-Dev"` |
| `ZED_RELEASE_CHANNEL` env var | `VOID_RELEASE_CHANNEL` |
| `ZED_APP_VERSION` env var | `VOID_APP_VERSION` |

### 2. `crates/paths/src/paths.rs`

Filesystem directories the app reads/writes to:

| Before | After |
|--------|-------|
| `Library/Application Support/Zed` (macOS) | `Library/Application Support/Void` |
| `Library/Logs/Zed` (macOS) | `Library/Logs/Void` |
| `.zed/` project local folder | `.void/` |
| `.zed/settings.json` | `.void/settings.json` |
| `.zed/tasks.json` | `.void/tasks.json` |
| `.zed/debug.json` | `.void/debug.json` |

Linux/Windows paths were already using `void` from a previous session.

### 3. `crates/zed/Cargo.toml`

Package metadata & macOS bundles:

| Before | After |
|--------|-------|
| `name = "zed"` | `name = "void"` |
| `version = "0.235.0"` | `version = "0.1.0"` |
| `authors = ["Zed Team <hi@zed.dev>"]` | `authors = ["Void Team"]` |
| `default-run = "zed"` | `default-run = "void"` |
| Binary: `zed` | Binary: `void` |
| Binary: `zed_visual_test_runner` | Binary: `void_visual_test_runner` |
| Bundle IDs: `dev.zed.Zed*` | Bundle IDs: `dev.void.Void*` |
| URL schemes: `zed://` | URL schemes: `void://` |

### 4. Root `Cargo.toml` â€” Build Fix

`copilot_ui` crate existed but was never registered in the workspace.
All 4 dependent crates (`zed`, `settings_ui`, `language_models`, `edit_prediction`, `edit_prediction_ui`)
referenced it as `copilot_ui.workspace = true` which caused build failure.

**Fixed:**
- Added `"crates/copilot_ui"` to `[workspace.members]`
- Added `copilot_ui = { path = "crates/copilot_ui" }` to `[workspace.dependencies]`

---

## âœ… Done Before This Session (Verified in void-docs)

| What | File | Notes |
|------|------|-------|
| Windows class names | `gpui_windows/src/window.rs` | `"Void::Window"` |
| Windows platform class | `gpui_windows/src/platform.rs` | `"Void::PlatformWindow"` |
| Agent ID | `agent/src/agent.rs` | `VOID_AGENT_ID` |
| Telemetry disabled | `assets/settings/default.json` | `diagnostics: false, metrics: false` |
| Sign-in hidden | Multiple files | `show_sign_in: false` |
| Welcome screen | `workspace/src/welcome.rs` | `"/void"`, `"The void awaits"` |
| Linux/Windows data dirs | `paths.rs` | Already `.join("void")` |

---

## ðŸ”´ What Still Needs Doing

### HIGH â€” User-visible, must fix before ship

| Task | Location | Notes |
|------|----------|-------|
| Replace `VectorName::ZedLogo` | `welcome.rs:482`, `ui/src/components/image.rs` | Needs new SVG asset |
| Menu "About Zed", help links | `zed/src/zed/app_menus.rs` | Whole menu section |
| Onboarding UI strings | `onboarding/src/**` | "Welcome to Zed" etc. |
| Settings UI strings | `settings_ui/src/page_data.rs` | Embedded "Zed" text |
| Error messages (`zed.dev` links) | `main.rs:139`, `zed.rs:607` | Linux troubleshooting URLs |

### MEDIUM â€” Runtime/functional impact

| Task | Location | Notes |
|------|----------|-------|
| Cloud API URLs | `http_client.rs:217-260` | `api.zed.dev`, `cloud.zed.dev` â€” will fail if cloud used |
| Provider ID | `language_model_core/src/provider.rs:19` | `ZED_CLOUD_PROVIDER_ID = "zed.dev"` |
| DB migration | `migrator.rs:1270` | `"provider": "zed.dev"` â€” misidentifies provider |
| OpenRouter HTTP header | `open_router/src/open_router.rs:450,543` | `HTTP-Referer: https://zed.dev` |
| Feedback email | `feedback.rs:37` | `hi@zed.dev` |

### LOW â€” Cosmetic / can wait

| Task | Location | Notes |
|------|----------|-------|
| 137 remaining `zed.dev` URL refs | See `void-remaining-urls.md` | Docs, test data, schema URLs |
| `.zed_server` remote path | `paths.rs:36` | SSH server dir name |
| Git default email | `git/repository.rs:3657` | `hi@zed.dev` test default |
| UI website buttons | `ui/components/button_link.rs:95` | "zed.dev" label |

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
