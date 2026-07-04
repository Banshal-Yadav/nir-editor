---
title: Nir on macOS
description: "Nir is built from source on macOS."
---

# Nir on macOS

Nir is built from source on macOS. See the [development guide](./development/macos.md) for build instructions.

## Uninstalling

Nir stores its data in the following directories on macOS:

- `~/.config/nir`
- `~/Library/Application Support/Nir`
- `~/Library/Caches/Nir`
- `~/Library/Logs/Nir`

Remove these to fully uninstall. If you installed the CLI, remove it with:

```sh
rm /usr/local/bin/nir
```

## Troubleshooting

### CLI command not found

If the `nir` command isn't available after building from source:

1. Check that the build output directory is in your PATH
2. Try reinstalling the CLI via {#action cli::InstallCliBinary} in the command palette
3. Open a new terminal window to reload your PATH

### Can't install CLI {#cant-install-cli}

{#action cli::InstallCliBinary} writes a `nir` symlink to `/usr/local/bin`, which requires administrator privileges. If your macOS account isn't in the `admin` group, the installer can't create that symlink.

Instead, add an alias pointing to the built `cli` binary to your shell configuration file (`~/.zshrc` for Zsh, `~/.bashrc` for Bash):

```sh
alias nir="/path/to/nir/target/release/nir"
```

After you restart your shell, you will be able to use `nir` from your terminal:

```sh
nir .              # Open current folder
nir file.txt       # Open a file
```

### GPU or rendering issues

Nir uses Metal for rendering on macOS. If you experience graphical glitches:

1. Ensure macOS is up to date
2. Restart your Mac to reset the GPU state
3. Check Activity Monitor for GPU pressure from other apps

### High memory or CPU usage

If Nir uses more resources than expected:

1. Check for runaway language servers in the terminal output ({#action zed::OpenLog})
2. Try disabling extensions one by one to identify conflicts
3. For large projects, consider using [project settings](./reference/all-settings.md#file-scan-exclusions) to exclude unnecessary folders from indexing

For additional help, see the [Troubleshooting guide](./troubleshooting.md) or visit the community.
