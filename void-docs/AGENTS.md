# /void Editor — Agent README

> Master reference for /void rebrand project. Start here before any work.
> **Last updated:** 2026-04-28 (session active)

## GPUI Examples Reference
All GPUI examples are in `crates/gpui/examples/` in this repo:
- Animation → animation.rs (for blinking cursor later)
- Opacity → opacity.rs (for fade effects)
- SVG → svg.rs (for logo)
- Input → input.rs (for search bar)

Before using any GPUI API, check examples/ first — no hallucination.

---

## 1. Project Overview
- Name: /void Editor
- Source: zed-industries/zed (GitHub)
- Location: C:\Users\bansa\OneDrive\Desktop\zed\zed
- Goal: Full rebrand Zed -> /void with tagline "THINK. BUILD. SHIP."
- Build: GitHub Actions (LNK2038 CRT mismatch in local builds)

---

## 2. Current Status

### ✅ Complete
- **64+ user-facing strings** replaced (Zed → /void)
- **Config dir:** `~/.zed/` → `~/.void/`
- **Window classes:** `Zed::Window` → `Void::Window`
- **Agent ID:** `ZED_AGENT_ID` → `VOID_AGENT_ID`
- **Sign-in UI disabled** in title_bar, cloud.rs, ai_onboarding
- **3 GitHub Actions workflows** created (debug, release, windows)
- **VoidLogo** simplified (no animation)
- **copilot_ui build fix** — Added to workspace members

### ⚠️ In Progress
- **Build verification** — waiting for GitHub Actions artifact

---

## Checkpoints (Working Milestones)
- Checkpoint 1: cb9512ef3d — feat(ui): implement Void Dark brutalist theme and minimal status bar (WORKING)
- Checkpoint 2: 1703db42a5 — feat(ui): add narrow left activity bar with project avatars (WORKING)

---

### ✅ Previously Fixed
- **Status Bar Overhaul** — Refactored `crates/workspace/src/status_bar.rs` to group non-essential items into a `...` overflow popover menu, preserving only Branch, File Type, and Cursor Position on the main bar.
- **Void Theme Integration** — Created `assets/themes/void/void.json` (brutalist `#0a0a0a` & amber `#ffb000` aesthetic) and set `DEFAULT_DARK_THEME` to `Void Dark` in `crates/theme/src/theme.rs`.
- **open_router.rs line 840** — Changed to `.bg(gpui::blue())` ✅
- **main.rs fail_to_open_window** — Fixed `_cx.background_spawn` → `_cx.spawn` ✅ (commit 31c3bfab3a)

### 🔴 Known Issues
- LNK2038 CRT mismatch: Local builds fail; use GitHub Actions
- Remaining URLs: 137+ zed.dev refs (see void-remaining-urls.md)
- Agent UI Complexity: Redesign in progress

---

## 3. Build & Test

### Fast Check (single crate)
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo check -p workspace
```

### Full Build (use GitHub Actions)
```powershell
# Push to trigger workflows, download artifact from Actions tab
git add .; git commit -m "check"; git push
```

### Push Workflow
1. Test compile: `cargo check -p <crate>`
2. Run sub-agent audit if major changes
3. Commit and push

---

## 4. Key Files Map
- workspace/src/welcome.rs: Welcome screen, VoidLogo
- paths/src/paths.rs: Config dir (~/.void/)
- gpui_windows/src/window.rs: Window class (Void::Window)
- language_models/src/provider/: AI providers (open_router, cloud)
- agent_ui/src/agent_panel.rs: Agent chat UI
- ai_onboarding/src/ai_onboarding.rs: AI onboarding
- settings_ui/src/page_data.rs: Settings UI strings
- .github/workflows/: Build workflows

### Reference Docs
- `void-docs/void-architecture.md` — Full crate documentation
- `void-docs/void-agent-layout.md` — Agent UI layout
- `void-docs/void-remaining-urls.md` — 137 voideditor.com URLs audit
- `void-docs/gpui-reference.md` — GPUI API cheat sheet (examples-derived)
- `void-docs/void-progress-report.md` — Detailed rebrand task breakdown
- `void-docs/DESIGN_SYSTEM.md` — Master Brutalist UI specs & icon sizes

---

## 5. Rules — DON'T TOUCH

### Never Change
- **Dependency crate names** in Cargo.toml (e.g., `gpui`, `zed*`)
- **Cargo.lock** (generated file)
- **Internal IDs:** `ZED_CLOUD_PROVIDER_ID`, `ZED_AGENT_ID`, `ZedModule`, `ZedHeapProvider`, `ZedPredictModal`
- **Provider configuration** unless explicitly asked

### Safe to Change
- User-facing strings (Zed → /void)
- Window class names
- Config directory names

### Before Modifying
1. Check `void-docs/void-architecture.md` for crate context
2. Check `void-docs/void-agent-layout.md` for agent UI
3. Test compile: `cargo check -p <crate>`

- **Never delete** void-docs/ folder or AGENTS.md
- **Never modify** .github/workflows/ without explicit instruction

---

## 6. Next Priorities (In Order)

1. **Test compile** — Run cargo check on language_models crate
2. **Verify build** — Download GitHub Actions artifact
3. **Run binary** — Test /void launches correctly
4. **URL cleanup** — Update void-remaining-urls.md progress
5. **Agent UI** — Future: simplify agent panel layout

---

## 7. Getting Help
- Crate architecture: void-docs/void-architecture.md
- Agent UI layout: void-docs/void-agent-layout.md
- GPUI API reference: void-docs/gpui-reference.md
- URL audit: void-docs/void-remaining-urls.md
- Detailed progress: void-docs/void-progress-report.md
- Design System: void-docs/DESIGN_SYSTEM.md
- Build errors: GitHub Actions logs
- Theme colors: theme/src/styles/colors.rs

## 8. Common Pitfalls & Troubleshooting

### 1. GPUI Async Spawning Syntax
Do **NOT** use `cx.background_spawn(async move { ... })` for background tasks inside the main UI crates. This is an old API or incorrect syntax.
- **Correct Syntax**: Use `cx.spawn(async move |_cx| { ... })` when you need an `AsyncApp` context, or `cx.background_executor().spawn(async move { ... })` for pure background tasks without context.

### 2. GitHub Actions Packaging Name (Zed vs. Void)
Because the binary was renamed from `zed` to `void` in the root and `crates/zed/Cargo.toml`, the GitHub Actions `.yml` files in `.github/workflows/` **MUST** reflect this when packaging artifacts.
- **Error**: `cp: cannot stat 'target/debug/zed': No such file or directory`
- **Fix**: The copy commands must use `cp target/debug/void void-package/` instead of `target/debug/zed`. If you rename binaries in Cargo.toml, ALWAYS check `.github/workflows/` for hardcoded artifact names.

---

*End of file*
