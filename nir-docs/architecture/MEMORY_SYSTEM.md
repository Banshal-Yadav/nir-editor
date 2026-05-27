# Memory System

> **Status:** ✅ IMPLEMENTED
> **Last updated:** 2026-05-28

---

## Overview

Nir features a persistent, Markdown-based memory system that allows the agent to remember context, goals, and preferences across sessions. Unlike traditional JSON-based systems, this memory is entirely human-readable and human-editable. Memories are stored as standard `.md` files in a centralized `~/.nir/brain/memory/` directory.

---

## Storage Architecture

All memory files are stored globally in your home directory (or `%APPDATA%` on Windows).

| Scope | Path (Windows) | Path (macOS/Linux) |
|---|---|---|
| **Core Memory** | `%APPDATA%\.nir\brain\memory\` | `~/.nir/brain/memory/` |

### The 5 Core Files

The system relies on 5 specific markdown files:

1. **`about.md`**: Core identity, background, and personal details. (Rarely updated).
2. **`settings.md`**: Tool rules, communication style, formatting preferences.
3. **`goals.md`**: Current focus, milestones, and active objectives.
4. **`projects.md`**: Codebase context, active directories, tech stacks.
5. **`bookmark.md`**: Ideas, links, prompts, resources, and things to try later.

### The "Working Notes" Section

Inside every file, the agent maintains a specific section headed by `## 📝 Working Notes`.
The agent will **only** read, write, modify, and delete timestamped entries under this section. Any text you write *above* the Working Notes section is treated as established, static context that the agent will read but never overwrite.

Example of an entry in the Working Notes section:
```markdown
## 📝 Working Notes

### [14:30:05] 2026-05-28 | ID: 2026-05-28-20260528T143005-1234
User prefers direct communication with no fluff.
```

---

## Pre-loading (System Prompt Injection)

To provide instant context without wasting tokens or API calls, Nir automatically injects the contents of **`about.md`** and **`settings.md`** directly into the system prompt at the start of every session.

The injected memories are formatted as:
```
### Memory Target: `about`
<contents of about.md>

### Memory Target: `settings`
<contents of settings.md>
```

This format is critical — it tells the LLM that `about` and `settings` are **Memory Targets** (to be interacted with via the `brain_memory` tool), not local files on disk. Using a `# ABOUT.MD` header caused the LLM to hallucinate a `read_file` call instead.

The agent instantly knows your identity and rules. The heavier files (`goals`, `projects`, `bookmark`) are **not** pre-loaded to save tokens. The agent must use the `brain_memory` tool to query them when needed.

---

## Tools (LLM-callable)

The agent has two powerful tools to interact with this system:

### `brain_memory`
**File:** [`crates/agent/src/tools/brain_memory_tool.rs`](../crates/agent/src/tools/brain_memory_tool.rs)

A Swiss-army knife tool for reading and writing memory files.
**Actions:**
- `create`: Appends a new timestamped entry to a file's Working Notes.
- `read`: Reads the Working Notes of a file (optionally filtered by ID or date).
- `read-many`: Reads the *entire* contents of multiple files at once.
- `modify`: Edits an existing entry by ID.
- `delete`: Removes an existing entry by ID.
- `list`: Lists all IDs and snippets in a file.

### `scratchpad`
**File:** [`crates/agent/src/tools/scratchpad_tool.rs`](../crates/agent/src/tools/scratchpad_tool.rs)

A lightweight tool for **temporary**, session-scoped notes. Unlike `brain_memory`, scratchpad entries are not meant to persist permanently — they are for mid-task checkpoints, raw data dumps, intermediate reasoning, and context that doesn't belong in long-term memory.
- Scratchpad file: `~/.nir/brain/scratch.md`
- Supports `create`, `read`, `modify`, `delete`, `list`, and `clear` actions.
- The agent is instructed never to output "Working Notes" headers directly in chat; it must use this tool instead.

### `brain_backup`
**File:** [`crates/agent/src/tools/backup_tool.rs`](../crates/agent/src/tools/backup_tool.rs)

A tool dedicated to safely backing up the markdown files before major modifications, and restoring them if something goes wrong.
- Backups are stored in `.backups/`.
- Includes support for backing up an entire `drafts` directory for generated content.
- Automatically handles timestamping and deduplication.

---

## How to Use It

### As a User

You can interact with the system naturally or explicitly:
- "Save my new tech stack to my projects memory."
- "What milestones do we have in our goals right now?"
- "Add a note to my settings that I hate markdown tables."
- "Forget the memory entry with ID 2026-05-28-..."

### Manually Editing

Because the system is pure Markdown, you can open `~/.nir/brain/memory/about.md` in any editor and type directly into it.
1. Add static context above the `## 📝 Working Notes` header.
2. Edit the AI's entries directly.
3. The agent will read your exact changes on the next turn.

---

## FAQ

**Q: Where did the old JSON system go?**
A: It was replaced entirely. The markdown system provides better transparency, easier manual editing, and eliminates the need for separate global vs project scopes by centralizing everything.

**Q: What if the files get too big?**
A: `about` and `settings` are pre-loaded, so you should manually prune them if they get massive. `goals`, `projects`, and `bookmarks` are fetched on-demand, so they can grow much larger without impacting every single chat.

**Q: Can I use this for non-coding notes?**
A: Absolutely! The `bookmark.md` file is explicitly designed for generic ideas, links, and scratchpad thoughts.
