# Memory System

> **Status:** ✅ IMPLEMENTED
> **Last updated:** 2026-05-27

---

## Overview

Nir has a persistent memory system that lets the agent remember things across conversations. Memories are stored on disk as append-only JSONL files and automatically injected into the system prompt at the start of every conversation — so the agent knows them without you having to repeat yourself.

---

## Storage

| Scope | Path (Windows) | Path (macOS/Linux) |
|---|---|---|
| **Global** | `%APPDATA%\.nir\memory.jsonl` | `~/.nir/memory.jsonl` |
| **Project** | `<project-root>\.nir\memory.jsonl` | `<project-root>/.nir/memory.jsonl` |

- **Global** — for personal preferences and info that apply everywhere (e.g. "I prefer tabs", "my name is X")
- **Project** — for project-specific context (e.g. "this project uses PostgreSQL", "run `pnpm dev` to start")

### File Format (JSONL)

Each memory is one JSON object per line:

```jsonl
{"id":"mem_1716834291000","content":"User prefers tabs over spaces","created_at":"2026-05-27T12:00:00Z"}
{"id":"mem_1716834299000","content":"Project uses Rust 2021 edition","created_at":"2026-05-27T12:01:00Z"}
```

When a memory is deleted, the file is rewritten and the line is completely removed. No traces or tombstone entries are left behind.

---

## Tools (LLM-callable)

The agent has three memory tools registered in [`crates/agent/src/tools.rs`](../crates/agent/src/tools.rs):

### `save_memory`
**File:** [`crates/agent/src/tools/memory_tool.rs`](../crates/agent/src/tools/memory_tool.rs) — `SaveMemoryTool`

Saves a piece of information to persistent memory.

```json
{
  "content": "User prefers functional React components",
  "scope": "global"
}
```

| Parameter | Type | Description |
|---|---|---|
| `content` | string | The text to remember. Be concise but complete. |
| `scope` | `"global"` \| `"project"` | Where to store it. Defaults to `"global"`. |

The agent calls this automatically when:
- You say "remember this" or "don't forget"
- You state a preference ("I always use TypeScript")
- It discovers a key project pattern or architecture detail
- You correct it about something

---

### `recall_memory`
**File:** [`crates/agent/src/tools/memory_tool.rs`](../crates/agent/src/tools/memory_tool.rs) — `RecallMemoryTool`

Searches saved memories. Useful mid-conversation to check something specific.

```json
{
  "query": "database",
  "scope": "all"
}
```

| Parameter | Type | Description |
|---|---|---|
| `query` | string | Case-insensitive substring search. Leave empty to list all. |
| `scope` | `"global"` \| `"project"` \| `"all"` | Which memories to search. Defaults to `"all"`. |

Returns a formatted list with IDs, timestamps, and content.

---

### `delete_memory`
**File:** [`crates/agent/src/tools/memory_tool.rs`](../crates/agent/src/tools/memory_tool.rs) — `DeleteMemoryTool`

Removes a memory completely by scrubbing it from the file (no traces left).

```json
{
  "id": "mem_1716834291000",
  "scope": "global"
}
```

| Parameter | Type | Description |
|---|---|---|
| `id` | string | The memory ID shown in `recall_memory` or the injected system prompt. |
| `scope` | `"global"` \| `"project"` | Where the memory lives. Defaults to `"global"`. |

The agent calls this when you say "forget that" or "that's no longer true".

---

## System Prompt Injection

**Every conversation**, both memory files are read from disk and injected into the system prompt automatically — no tool call needed. The agent always starts knowing what's been saved.

### Where it's injected

In [`system_prompt.hbs`](../crates/agent/src/templates/system_prompt.hbs), a `## Your Memory` section appears before the User's Custom Instructions block:

```
## Your Memory

The following memories were saved from previous conversations...

### Global (all projects)
- User prefers tabs over spaces (id: mem_1716834291000)

### Project-specific
- Project uses PostgreSQL with Prisma ORM (id: mem_1716834299000)
```

### Limits

| Limit | Value |
|---|---|
| Max entries injected per scope | 50 |
| Max characters injected per scope | 4,000 |
| Truncation notice | Shown if limit hit |

Entries are shown oldest-first (most established context first). Each entry includes its `id` so the agent can call `delete_memory` if needed.

---

## Implementation Files

| File | Role |
|---|---|
| [`crates/agent/src/tools/memory_tool.rs`](../crates/agent/src/tools/memory_tool.rs) | All three tool implementations + storage helpers |
| [`crates/agent/src/tools.rs`](../crates/agent/src/tools.rs) | Tool registration in the `tools!` macro |
| [`crates/agent/src/templates.rs`](../crates/agent/src/templates.rs) | `SystemPromptTemplate` struct — holds `global_memories` and `project_memories` fields |
| [`crates/agent/src/thread.rs`](../crates/agent/src/thread.rs) | Reads memory files and passes them to the template in `build_request_messages()` |
| [`crates/agent/src/templates/system_prompt.hbs`](../crates/agent/src/templates/system_prompt.hbs) | Renders the `## Your Memory` section |

---

## How to Use It

### As a user

Just talk naturally. The agent will save things on its own. You can also be explicit:

- "Remember that I prefer snake_case for Python variables."
- "Don't forget — we use `pnpm`, not `npm`."
- "Save this to global memory: my timezone is IST."
- "Forget the memory about tabs."
- "What do you remember about this project?"

### Manually editing memories

The JSONL files are plain text. You can open them and add entries by hand:

```jsonl
{"id":"mem_manual_001","content":"Team standup is at 10am IST","created_at":"2026-05-27T00:00:00Z"}
```

Or delete entries by removing the line from the file.

The agent will pick up your manual edits on the next conversation.

---

## FAQ

**Q: Does the agent automatically save everything?**  
No — the docstrings on the tools tell it *when* to save (preferences, corrections, "remember this"). It won't blindly log every message.

**Q: Can I see what's saved?**  
Yes — ask "what do you remember?" and the agent will call `recall_memory`. Or just open the JSONL files directly.

**Q: What happens if the memory file doesn't exist?**  
`read_memories()` returns an empty list. No error. The first `save_memory` call creates the directory and file automatically.

**Q: Is there a UI for memories?**  
Not yet. It's all tool-based and file-based for now.
