# Upstream Merge — 2026-06-16

Tracking the merge from `zed-industries/zed` (upstream) into `Banshal-Yadav/nir` (origin).

| Field | Value |
|-------|-------|
| **Upstream ref** | `137e677a05..6661273a41` (185 commits, Jun 16) |
| **Upstream URL** | `https://github.com/zed-industries/zed.git` |
| **Local branch** | `main` |
| **Snapshot** | `pre-merge-snapshot-2026-06-16` at `301d1593a1` |

## Pre-merge State

- [x] All local work committed
- [x] Snapshot branch created and pushed
- Last local commit: `301d1593a1 feat(agent): improve system prompt with workflow guidance and memory rules`
- 4 commits ahead of last merge

## Key Upstream Features Brought In

- **Context compaction**: `/compact` command, auto-compact settings, refined UX, compaction telemetry
- **Linux sandboxing**: bubblewrap integration, network allowlist (allow/deny per host), sandbox permission prompts
- **HTTP proxy crate**: in-process allowlisting proxy server with upstream proxy config
- **Skills management**: moved from agent into settings UI
- **Anthropic-compatible provider**: additional LLM provider support
- **Stream git blame parsing**: async blame with streaming chunks
- **Workspace typed errors**: `workspace_error.rs` for typed error handling
- **Benchmarks**: reorganized to `crates/benchmarks/`
- OpenCode model updates, general bug fixes, UI polish

## Conflicts Resolved (34 files)

| Category | Strategy |
|----------|----------|
| GitHub workflows (11) | Took upstream (restored deleted CI files) |
| Agent core (5 files) | Kept Nir features + restored deleted tools |
| Agent UI (2 files) | Kept Nir version (full agent launcher/analytics) |
| Settings (4 files) | Kept Nir defaults + upstream code merged |
| LLM provider (1 file) | Kept Nir branding + upstream code |
| Workspace/UI (4 files) | Merged both |
| Branding/docs (5 files) | Took upstream + reapplied Nir branding |
| Simple/misc (2 files) | Took upstream (no Nir changes) |

## Post-merge Fixes (follow-up commit, not yet committed)

### Plan Tool — Restored & Wired

Upstream deleted `update_plan` / `update_title` as "experimental" (#58824).
Fully restored across all 4 gating layers:

- `tools!` macro, `add_default_tools()`, settings allowlists
- `ThreadEvent::Plan` → now calls `acp_thread.update_plan()` (was no-op)
- Added proper LLM descriptions

### delete_log_entry — New Tool

Removes a log entry from markdown file + FTS5 index atomically.
`recall_past_context` now exposes entry IDs for targeting.

### Tool Description Cleanup

| Tool | Change |
|------|--------|
| `scratchpad` | Long defensive → "Use PROACTIVELY, reading is free, forgetting costs tokens" |
| `log_task_completion` | Explicit allow/deny list (no more "Acknowledged user input" spam) |
| `update_plan` | Added description (was empty) |
| `update_title` | Added description (was empty) |

### Other

- Fixed `EXCLUDED_TOOLS`: `"backup"` → `"brain_backup"`, added missing Nir tools
- Removed dead `skill_creator` code from `agent_panel.rs`
- Scrapped `update_log_entry` tool (implemented but user decided against it)

## Status

- [x] Merge auto-resolution done
- [x] 34 conflicts resolved
- [x] Plan tool restored and tested
- [x] delete_log_entry works (confirmed)
- [x] `cargo build` passes (debug)
- [ ] Follow-up commit with post-merge fixes (pending user approval)
- [ ] Push to `origin/main`
