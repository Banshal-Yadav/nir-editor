---
title: Outline Panel - Zed
description: Navigate code structure with Zed's outline panel. View symbols, jump to definitions, and browse file outlines.
---

# Outline Panel

In addition to the modal outline (`cmd-shift-o`), Zed offers an outline panel. The outline panel can be deployed via `cmd-shift-b` ({#action outline_panel::ToggleFocus} via the command palette), or by clicking the `Outline Panel` button in the status bar.

When viewing a "singleton" buffer (i.e., a single file on a tab), the outline panel works similarly to that of the outline modal－it displays the outline of the current buffer's symbols. Each symbol entry shows its type prefix (such as "struct", "fn", "mod", "impl") along with the symbol name, helping you quickly identify what kind of symbol you're looking at. Clicking on an entry allows you to jump to the associated section in the file. The outline view will also automatically scroll to the section associated with the current cursor position within the file.

![Using the outline panel in a singleton buffer](https://github.com/Banshal-Yadav/nir