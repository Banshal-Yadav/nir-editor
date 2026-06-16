# Upstream Merge — 2026-06-16

Tracking today's merge from `zed-industries/zed` (upstream) into `Banshal-Yadav/nir` (origin).

## Pre-merge State

- [x] All local work committed
- [x] Snapshot branch: `pre-merge-snapshot-2026-06-16` at `301d1593a1`
- [x] Pushed to `origin/main`
- Last local commit: `301d1593a1 feat(agent): improve system prompt with workflow guidance and memory rules`
- 4 commits ahead of last merge (2 fixes + 1 feat)
- Upstream: 185 commits ahead (`137e677a05..6661273a41`)

## Merge Steps

- [x] `git fetch upstream` (done)
- [x] `git merge upstream/main`
- [x] Resolve conflicts
- [ ] `cargo build` (debug — skipped, no Rust toolchain in env)
- [ ] Smoke test core features (chat, agent, terminal)
- [ ] Test Nir-specific tools

## Key Upstream Features Preserved

- **Compaction**: `/compact` command, auto-compact settings, server-side (OpenAI/Anthropic)
- Stream git blame parsing
- Skills management in settings UI
- OpenCode model updates
- Typed workspace errors (`workspace_error.rs`)
- Sandboxing improvements (bubblewrap)
- Benchmarks crate moved to `crates/benchmarks/`
- Anthropic-compatible provider
- HTTP proxy crate

## Conflicts Resolved (34 files total)

| Category | Files | Strategy |
|----------|-------|----------|
| GitHub workflows (11) | `.github/workflows/*` | Took upstream (restored deleted CI files) |
| Agent core (5) | `thread.rs`, `system_prompt.hbs`, `experimental_system_prompt.hbs`, `tools.rs` | Kept Nir features + restored deleted tools |
| Agent UI (2) | `agent_panel.rs`, `message_editor.rs` | Kept Nir version (full agent launcher/analytics) |
| Settings (4) | `default.json`, `skills_setup.rs`, `tool_permissions_setup.rs`, `settings_ui.rs` | Kept Nir defaults + branding |
| LLM provider (1) | `open_ai_compatible.rs` | Kept Nir branding + upstream code |
| Workspace/UI (4) | `workspace.rs`, `welcome.rs`, `notifications.rs`, `title_bar.rs` | Merged: kept Nir welcome/title, took upstream notifications, merged both modules |
| Branding/docs (5) | `askpass.rs`, `gpui/README.md`, `install_cli.rs`, `mcp.md`, `linux.md` | Took upstream + reapplied Nir branding |
| Simple/misc (2) | `zed_urls.rs`, `zeta.rs` | Took upstream (no Nir changes) |

## Post-merge

- [ ] `cargo build` smoke test (needs Rust toolchain)
- [ ] Push merged branch to `origin/main`
- [ ] Log merge commit SHA
