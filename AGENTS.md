# AGENTS.md - /void Editor (Rebranded Zed)

# /void Agent Instructions

## Architecture Reference
See void-docs/void-architecture.md for crate documentation.
See void-docs/void-agent-layout.md for agent UI layout.
See void-docs/void-remaining-urls.md for pending URL replacements.

## Documentation Files (void-docs/)
| File | Purpose |
|------|---------|
| void-docs/void-architecture.md | Crate documentation, verified changes, file contents |
| void-docs/void-agent-layout.md | Current vs proposed agent UI layout, GPUI components |
| void-docs/void-remaining-urls.md | 137+ zed.dev URLs audit, cleanup action items |

## Agent Rules (MANDATORY for all agents)

### Before Modifying Code
1. **Always check void-docs/void-architecture.md** before modifying any crate
2. **Always check void-docs/void-agent-layout.md** before touching agent UI
3. **Use void-docs/void-remaining-urls.md** to track URL cleanup progress
4. **Never break existing providers** in language_models crate (keep ZED_CLOUD_PROVIDER_ID, etc.)
5. **Test compile before pushing** — check for unused imports and type errors

### Build & Push
- Test compile: `cargo check -p <crate>` before commit
- Never push broken builds — fix compile errors first
- If local build fails, use GitHub Actions workflows

### Safe Changes
- User-facing strings: OK to replace Zed → /void
- Internal IDs (ZED_AGENT_ID, ZED_CLOUD_PROVIDER_ID): DO NOT CHANGE
- Provider configuration: DO NOT CHANGE unless explicitly asked
- Window classes: Safe to rename (Void::Window)

## Project Overview
- **Name:** /void Editor (formerly "Zed")
- **Source Repo:** zed-industries/zed (GitHub)
- **Local Location:** `C:\Users\bansa\OneDrive\Desktop\zed\zed`
- **Goal:** Full rebrand from "Zed" to "/void" with tagline "The void awaits"
- **Build Method:** GitHub Actions (worked after local build failures)

---

## Rebrand Status: ✅ COMPLETE

### Strings Changed (64+ user-facing strings across 25+ files)

| Category | Changes |
|----------|---------|
| User config dir | `~/.zed/` → `~/.void/` |
| Log files | `Zed.log` → `void.log` |
| Window classes | `Zed::Window` → `Void::Window`, `Zed::PlatformWindow` → `Void::PlatformWindow` |
| Agent ID | `ZED_AGENT_ID` → `VOID_AGENT_ID`, "Zed Agent" → "/void Agent" |
| AI strings | "Sign In to use Zed AI" → "/void AI", "Zed's hosted models" → "/void hosted models" |
| Error messages | "Zed failed to launch" → "/void failed to launch" |
| Settings UI | "Zed" → "/void" in all descriptions |
| Version strings | "Zed/{}" → "/void/{}" |
| Keybind context | "Zed Keybind Context" → "/void Keybind Context" |
| Subscription UI | All upgrade/sign-in buttons disabled (set to false) |
| Title bar | Sign-in button hidden by default |
| Welcome screen | VoidLogo, tab text "/void", IconName::Ai |

### Key Files Modified
- `crates/paths/src/paths.rs` — config dir → void
- `crates/gpui_windows/src/window.rs` — window class → Void::Window
- `crates/gpui_windows/src/platform.rs` — platform window class
- `crates/agent/src/agent.rs` — VOID_AGENT_ID
- `crates/language_models/src/provider/cloud.rs` — AI subscription UI disabled
- `crates/title_bar/src/title_bar.rs` — sign-in button hidden
- `crates/ai_onboarding/src/ai_onboarding.rs` — sign-in UI disabled
- `crates/settings_ui/src/settings_ui.rs` — keybind context renamed
- `crates/keymap_editor/src/keymap_editor.rs` — keybind context renamed
- `assets/settings/default.json` — show_sign_in: false
- `crates/workspace/src/welcome.rs` — VoidLogo, "/void" tab text, IconName::Ai
- `.github/workflows/` — build-debug.yml, build-release.yml, build-windows.yml

---

## Sign-In Features Disabled

| Feature | Status | Method |
|---------|--------|--------|
| Title bar Sign In button | ✅ Disabled | `show_sign_in: false` in defaults, UI hidden in title_bar.rs |
| AI configuration sign-in | ✅ Disabled | Conditional set to `false` in cloud.rs |
| Subscription upgrade buttons | ✅ Disabled | Conditionals set to `false` |
| AI onboarding sign-in | ✅ Disabled | Always show free plan state |
| Account URLs | ⚠️ Keep as no-op | URLs still exist but unreachable from UI |

---

## Build Status: ✅ COMPLETE (via GitHub Actions)

### Local Build Attempts (Failed)
- **Debug build:** Failed - spectre libs panic
- **After spectre patch:** Failed - LNK2038 (CRT mismatch) - `libwebrtc_sys` compiled with `/MT` (static), Zed with `/MD` (dynamic)
- **Release build:** Failed - same LNK2038 + laptop turned off multiple times

### Root Cause (Local Build)
```
error LNK2038: RuntimeLibrary mismatch - MT_StaticRelease vs MD_DynamicRelease
```

### Final Solution
- **GitHub Actions** - Built successfully on CI
- Created 3 workflow files:
  - `build-debug.yml` - Debug build on ubuntu-latest
  - `build-release.yml` - Release build on ubuntu-latest  
  - `build-windows.yml` - Windows build on windows-latest

---

## System Specs
| | Value |
|--------|-------|
| OS | Windows 11 |
| Shell | PowerShell |
| VS | VS2022 + Spectre libs |
| Rust | 1.85+ (stable-x86_64-pc-windows-msvc) |

---

## Key Patches Made

### Session 2026-04-28 Updates
**Files changed:** welcome.rs, ai_onboarding.rs
**Commit:** `01f1399801`

| Change | File | Details |
|--------|------|---------|
| Simplify VoidLogo | welcome.rs | Removed animation, blinking cursor — static "/void" + "the void awaits" |
| Clean imports | welcome.rs | Removed Animation, AnimationExt, pulsating_between, Vector, VectorName |
| Fix unused variable | welcome.rs | Removed `_welcome_label` variable (was unused) |
| Dead code allow | ai_onboarding.rs | Added `#[allow(dead_code)]` to render_trial_state, render_pro_plan_state, render_business_plan_state, render_student_plan_state |

### Spectre libs patch
**File:** `msvc_spectre_libs-0.1.3\build.rs`
- Replaced panic with warning, allows build to continue

### welcome.rs compile fixes
**File:** `crates/workspace/src/welcome.rs`
- Line 36: "Zed welcome screen" → "/void welcome screen"
- Line 397: IconName::ZedAssistant → IconName::Ai
- Line 566: "Welcome".into() → "/void".into()
- VoidLogo component - removed .font() calls, fixed Duration::from_secs

---

## Next Steps
1. Download the GitHub Actions artifact (.exe)
2. Test /void rebrand by running the binary
3. Verify all strings display correctly
4. Ship /void!

---

## Commands Reference

### Local Build (release)
```powershell
cd C:\Users\bansa\OneDrive\Desktop\zed\zed
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo build -p zed --release --jobs 1
```

### Check disk space
```powershell
Get-PSDrive -Name C | Select-Object @{N='Free(GB)';E={[math]::Round($_.Free/1GB,2)}}
```

---

## Lessons Learned
- **LNK2038 = prebuilt binary mismatch** - can't fix locally without matching CRT libs
- **GitHub Actions = reliable Windows builds** - use CI for complex native projects
- **Spectre libs = VS component** - install via VS Installer, not cargo
- **Sign-in features = UI conditionals** - disable by setting conditions to false

(End of file - total 131 lines)