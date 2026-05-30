# Critical Links & Values — /nir

> [!CAUTION]
> These are **internal routing keys and API endpoints** — not user-facing branding strings.
> Changing them without understanding the downstream effects will silently break core features
> with **no compile error**. Read this before any branding audit or URL sweep.

---

## 1. `server_url` — The Most Dangerous Setting

**File:** [`assets/settings/default.json`](../assets/settings/default.json) (line ~2556)

```json
"server_url": "https://zed.dev"
```

### Why it must stay `"https://zed.dev"`

`server_url` is **not a display string**. It is the **switch key** used in
`crates/http_client/src/http_client.rs` to route API calls to the correct subdomains:

```rust
// crates/http_client/src/http_client.rs
match base_url.as_ref() {
    "https://zed.dev"         => "https://api.zed.dev",           // Extensions marketplace
    "https://staging.zed.dev" => "https://api-staging.zed.dev",
    "http://localhost:3000"   => "http://localhost:8080",
    other                     => other,  // ← Fallback: uses URL as-is
}
```

If `server_url` is set to anything other than `"https://zed.dev"`, the `other` arm fires
and the app tries to call e.g. `https://github.com/Banshal-Yadav/nir/extensions/...` — a
GitHub HTML page — instead of the real API.

### What breaks when changed

| Feature | Endpoint | Broken |
|---------|----------|--------|
| Extensions marketplace | `api.zed.dev/extensions` | ✅ Yes |
| Sign in / user profile | `cloud.zed.dev/client/users/connect` | ✅ Yes |
| Auth token refresh | `cloud.zed.dev/client/users/me` | ✅ Yes |
| System settings sync | `cloud.zed.dev/client/system_settings` | ✅ Yes |

### History

On **2026-05-30**, the branding audit replaced this with `"https://github.com/Banshal-Yadav/nir"`.
This broke extensions and sign-in silently. Fixed in commit `c203771cc8`.

---

## 2. `http_client.rs` — Endpoint Routing Functions

**File:** [`crates/http_client/src/http_client.rs`](../crates/http_client/src/http_client.rs)

Three functions control where network calls go. Their current state:

| Function | Status | Routes to | Used by |
|----------|--------|-----------|---------|
| `build_zed_api_url` | ✅ **Active** | `api.zed.dev` | Extensions marketplace |
| `build_zed_cloud_url` | ✅ **Active** | `cloud.zed.dev` | Sign in, auth, user profile |
| `build_zed_cloud_url_with_query` | ✅ **Active** | `cloud.zed.dev` | Auth queries, auto-update |
| `build_zed_llm_url` | ❌ **Disabled** | `""` (blank) | Zed AI LLM, edit prediction, web search |

> [!IMPORTANT]
> `build_zed_llm_url` is intentionally disabled. It powers Zed's cloud AI subscription
> (edit prediction, `zed.dev` LLM completions). Keep it blanked out.

> [!WARNING]
> `build_zed_api_url` is shared between **extensions** and **telemetry**.
> Telemetry is separately gated in `crates/client/src/telemetry.rs` at line ~520:
> ```rust
> if !state.settings.metrics { return; }
> ```
> As long as `"metrics": false` is set in default settings, telemetry won't fire
> even though the URL resolves correctly.

---

## 3. Telemetry Settings — Keep Disabled

**File:** [`assets/settings/default.json`](../assets/settings/default.json)

```json
"telemetry": {
  "diagnostics": false,
  "metrics": false
}
```

These must remain `false`. The code gate is in `telemetry.rs`:

```rust
fn report_event(...) {
    if !state.settings.metrics { return; }  // ← gate
    // ... sends to api.zed.dev/telemetry/events
}
```

---

## 4. Safe to Change vs. Dangerous

### ✅ Safe to rebrand (display strings, doc links)
- `DOCS_URL` in `crates/zed/src/zed.rs` — shown in UI, no API dependency
- `STATUS_URL` in `crates/zed/src/zed.rs` — status page link
- `ZED_REPL_DOCUMENTATION` — opens in browser, no API
- Comment URLs throughout the codebase
- Theme schema `$schema` URLs in `assets/themes/**/*.json`
- Keymap documentation URLs in `assets/keymaps/**/*.json`

### ❌ Do NOT change without understanding routing impact
- `"server_url"` in `assets/settings/default.json`
- Match arms in `build_zed_api_url`, `build_zed_cloud_url`, `build_zed_cloud_url_with_query`
- The `"https://api.anthropic.com"`, `"https://generativelanguage.googleapis.com"` etc. in `language_models` section of default.json — those are actual third-party API endpoints

---

## 5. Collab / RPC Server — Kept Disabled

The collab/RPC server (`wss://...`) is a separate system. `/nir` does not run its own
collab backend. The `build_zed_rpc_url` call in `client.rs` uses `ZED_RPC_URL` env var,
which is unset in production builds — so collab features are effectively no-ops.

---

## Quick Diagnosis

If extensions stop loading or sign-in disappears:

1. Check `"server_url"` in `assets/settings/default.json` — must be `"https://zed.dev"`
2. Check `build_zed_api_url` match arm for `"https://zed.dev"` — must point to `"https://api.zed.dev"`
3. Check `build_zed_cloud_url` match arm for `"https://zed.dev"` — must point to `"https://cloud.zed.dev"`
4. Check `telemetry.metrics` in default.json — must be `false`

_Last updated: 2026-05-30 after branding audit incident (commit `c203771cc8`)_
