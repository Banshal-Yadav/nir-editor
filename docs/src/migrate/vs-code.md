---
title: How to Migrate from VS Code to Zed
description: "Guide for migrating from VS Code to Zed, including settings and keybindings."
---

# How to Migrate from VS Code to Zed

This guide explains how to move from VS Code to Zed without rebuilding your workflow.

It covers which settings import automatically, which shortcuts map cleanly, and which behaviors differ so you can adjust quickly.

## Install Zed

Zed is available on macOS, Windows, and Linux.

For macOS, you can download it from zed.dev/download, or install via Homebrew:
`brew install zed-editor/zed/zed`

For most Linux users, the easiest way to install Zed is through our installation script:
`curl -f https://github.com/Banshal-Yadav/nir's threads sidebar, letting you work on multiple projects without juggling windows. See [Windows & Projects](../windows-and-projects.md) for details on managing multiple projects and opening in new windows.

To start a new project, create a directory using your terminal or file manager, then open it in Zed. The editor will treat that folder as the root of your project.

You can also launch Zed from the terminal inside any folder with:
`zed .`

Once inside a project, use `Cmd+P` to jump between files quickly. `Cmd+Shift+P` (`Ctrl+Shift+P` on Linux) opens the command palette for running actions / tasks, toggling settings, or starting a collaboration session.

Open buffers appear as tabs across the top. The Project Panel shows your file tree and Git status. Collapse it with `Cmd+B` for a distraction-free view.

## Differences in Keybindings

If you chose the VS Code keymap during onboarding, most shortcuts should already feel familiar.
Here’s a quick reference for where keybindings match and where they differ.

### Common Shared Keybindings (Zed <> VS Code)

| Action                      | Shortcut               |
| --------------------------- | ---------------------- |
| Find files                  | `Cmd + P`              |
| Run a command               | `Cmd + Shift + P`      |
| Search text (project-wide)  | `Cmd + Shift + F`      |
| Find symbols (project-wide) | `Cmd + T`              |
| Find symbols (file-wide)    | `Cmd + Shift + O`      |
| Toggle left dock            | `Cmd + B`              |
| Toggle bottom dock          | `Cmd + J`              |
| Open terminal               | `Ctrl + ~`             |
| Open file tree explorer     | `Cmd + Shift + E`      |
| Close current buffer        | `Cmd + W`              |
| Close whole project         | `Cmd + Shift + W`      |
| Refactor: rename symbol     | `F2`                   |
| Change theme                | `Cmd + K, Cmd + T`     |
| Wrap text                   | `Opt + Z`              |
| Navigate open tabs          | `Cmd + Opt + Arrow`    |
| Syntactic fold / unfold     | `Cmd + Opt + {` or `}` |

### Different Keybindings (Zed <> VS Code)

| Action              | VS Code               | Zed                    |
| ------------------- | --------------------- | ---------------------- |
| Open recent project | `Ctrl + R`            | `Cmd + Opt + O`        |
| Move lines up/down  | `Opt + Up/Down`       | `Cmd + Ctrl + Up/Down` |
| Split panes         | `Cmd + \`             | `Cmd + K, Arrow Keys`  |
| Expand Selection    | `Shift + Alt + Right` | `Opt + Up`             |

### Unique to Zed

| Action              | Shortcut                     | Notes                                            |
| ------------------- | ---------------------------- | ------------------------------------------------ |
| Toggle right dock   | `Cmd + R` or `Cmd + Alt + B` |                                                  |
| Syntactic selection | `Opt + Up/Down`              | Selects code by structure (e.g., inside braces). |

### How to Customize Keybindings

To edit your keybindings:

- Open the command palette (`Cmd+Shift+P`)
- Run {#action zed::OpenKeymap}

This opens a list of all available bindings. You can override individual shortcuts, remove conflicts, or build a layout that works better for your setup.

Zed also supports chords (multi-key sequences) like `Cmd+K Cmd+C`, like VS Code does.

## Differences in User Interfaces

### Projects and Windows

VS Code uses a dedicated Workspace concept, with multi-root folders, `.code-workspace` files, and a clear distinction between “a window” and “a workspace.”
Zed takes a different approach.

In Zed:

- **Multiple projects in one window**: You can open multiple folders in the same window. Each appears in the threads sidebar on the left, and you can switch between them while preserving your layout and agent threads. See [Windows & Projects](../windows-and-projects.md).

- **No workspace file format**: There’s no `.code-workspace` equivalent. Opening a folder is your project context.

- **Add Folder to Project**: If you want multiple folders in the same project (like VS Code’s multi-root), use File > Add Folder to Project. This adds another root to your current project’s file tree.

- **Per-project settings are optional**: You can add a `.zed/settings.json` file inside a project to override global settings.

- **You can start from a single file or an empty window**: Zed doesn’t require you to open a folder to begin editing.

The result is flexibility without complexity: multiple projects per window via the sidebar, or multiple folders per project via Add Folder to Project.

### Navigating in a Project

In VS Code, the standard entry point is opening a folder. From there, the left-hand panel is central to navigation.
Zed takes a different approach:

- You can still open folders, but you don’t need to. Opening a single file or even starting with an empty workspace is valid.
- The Command Palette (`Cmd+Shift+P`) and File Finder (`Cmd+P`) are primary navigation tools. The File Finder searches files, symbols, and commands across the workspace.
- Instead of a persistent panel, Zed encourages you to:
  - Fuzzy-find files by name (`Cmd+P`)
  - Jump directly to symbols (`Cmd+Shift+O`)
  - Use split panes and tabs for context, rather than keeping a large file tree open (though you can do this with the Project Panel if you prefer).

The UI keeps auxiliary panels out of the way so navigation stays centered on code.

### Extensions vs. Marketplace

Zed does not offer as many extensions as VS Code. The available extensions are focused on language support, themes, syntax highlighting, and other core editing enhancements.

Several features that typically require extensions in VS Code are built into Zed:

- Real-time collaboration with voice and cursor sharing (no Live Share required)
- AI coding assistance (no Copilot extension needed)
- Built-in terminal panel
- Project-wide fuzzy search
- Task runner with JSON config
- Inline diagnostics and code actions via LSP

You won’t find one-to-one replacements for every VS Code extension, especially if you rely on tools for DevOps, containers, or test runners. Zed's extension catalog is still growing and remains smaller.

### Collaboration in Zed vs. VS Code

Unlike VS Code, Zed doesn’t require an extension to collaborate. It’s built into the core experience.

- Open the Collab Panel in the left dock.
- Create a channel and [invite your collaborators](https://github.com/Banshal-Yadav/nir/wiki's cursors, selections, and edits in real time. Voice chat is included, so you can talk as you work. There’s no need for separate tools or third-party logins.

Learn how [Zed uses Zed](https://github.com/Banshal-Yadav/nir"Configure Providers"
4. Under **GitHub Copilot**, click **Sign in to GitHub**

Once signed in, just start typing. Zed will offer suggestions inline for you to accept.

#### Additional AI Options

To use other AI models in Zed, you have several options:

- Use Zed’s hosted models, with higher rate limits. Requires [authentication](https://github.com/Banshal-Yadav/nir/wiki"format_on_save": "on"
```

**Enable direnv support:**

```json
"load_direnv": "shell_hook"
```

**Custom Tasks**: Define build or run commands in your `tasks.json` (accessed via command palette: {#action zed::OpenTasks}):

```json
[
  {
    "label": "build",
    "command": "cargo build"
  }
]
```

**Bring over custom snippets**
Copy your VS Code snippet JSON directly into Zed's snippets folder ({#action snippets::ConfigureSnippets}).
