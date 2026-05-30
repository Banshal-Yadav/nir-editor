---
title: Release Notes
description: "Guide to release notes for /nir development."
---

# Release Notes

Whenever you open a pull request, the body is automatically populated based on this [pull request template](https://github.com/zed-industries/zed/blob/main/.github/pull_request_template.md).

```md
...

Release Notes:

- N/A _or_ Added/Fixed/Improved ...
```

On Wednesdays, we run [`get-preview-channel-changes`](https://github.com/zed-industries/zed/blob/main/script/get-preview-channel-changes), which collects `Release Notes` lines from pull requests landing in preview, as described in the [Release](https://github.com/Banshal-Yadav/nir/wiki't assume a user knows technical editor developer lingo; phrase your change in language they understand as a user of a text editor.
- If you want to include technical details about your pull request for other team members to see, do so above the `Release Notes` line.
- Changes to docs should be labeled as `N/A`.
- If your pull request adds/changes a setting or a keybinding, always mention that setting or keybinding.
  Don't make the user dig into docs or the pull request to find this information (although it should be included in docs as well).
- For pull requests that are reverts:
  - If the item being reverted **has already been shipped**, include a `Release Notes` line explaining why we reverted, as this is a breaking change.
- If the item being reverted **hasn't been shipped**, edit the original PR's `Release Notes` line to `N/A`; otherwise, it will still be included and the release notes compiler may not know to skip it.
