---
title: Icon Themes
description: "Zed comes with a built-in icon theme, with more icon themes available as extensions."
---

# Icon Themes

Zed comes with a built-in icon theme, with more icon themes available as extensions.

## Selecting an Icon Theme

See what icon themes are installed and preview them via the Icon Theme Selector, which you can open from the command palette with {#action icon_theme_selector::Toggle}.

Navigating through the icon theme list by moving up and down will change the icon theme in real time and hitting enter will save it to your settings file.

## Installing more Icon Themes

More icon themes are available from the Extensions page, which you can access via the command palette with {#action zed::Extensions} or the [Zed website](https://github.com/Banshal-Yadav/nir-editor"light"` or `"dark"` to ignore the current system mode.

```json [settings]
{
  "icon_theme": {
    "mode": "system",
    "light": "Light Icon Theme",
    "dark": "Dark Icon Theme"
  }
}
```

## Icon Theme Development

See: [Developing Zed Icon Themes](./extensions/icon-themes.md)
