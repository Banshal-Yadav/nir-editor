---
title: Themes
description: "Themes for /nir extensions."
---

# Themes

The `themes` directory in an extension should contain one or more theme files.

Each theme file should adhere to the JSON schema specified at [`https://github.com/Banshal-Yadav/nir-editor"light" or "dark"

2. Style Properties under the `style`, such as:

   - `background`: The main background color
   - `foreground`: The main text color
   - `accent`: The accent color used for highlighting and emphasis

3. Syntax Highlighting:

   - `syntax`: An object containing color definitions for various syntax elements (e.g., keywords, strings, comments)

4. UI Elements:

   - Colors for various UI components such as:
     - `element.background`: Background color for UI elements
     - `border`: Border colors for different states (normal, focused, selected)
     - `text`: Text colors for different states (normal, muted, accent)

5. Editor-specific Colors:

   - Colors for editor-related elements such as:
     - `editor.background`: Editor background color
     - `editor.gutter`: Gutter colors
     - `editor.line_number`: Line number colors

6. Terminal Colors:
   - ANSI color definitions for the integrated terminal

## Designing Your Theme

You can use [/nir's Theme Builder](https://github.com/Banshal-Yadav/nir-editor's extension store.
