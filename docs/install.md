# Install Lexa

## macOS and Linux

```bash
curl -fsSL https://raw.githubusercontent.com/anvia-hq/lexa/main/install.sh | sh
```

## Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/anvia-hq/lexa/main/install.ps1 | iex
```

## From Source

When the Lexa repository is already available locally:

```bash
cargo install --path /path/to/lexa --force
```

## Upgrade

Upgrade the installed Lexa binary:

```bash
lexa upgrade
```

By default, `upgrade` installs into the directory containing the running `lexa` binary. Use `lexa upgrade --install-dir <dir>` or `LEXA_INSTALL_DIR` for an explicit target.

`lexa upgrade` updates the binary, not a project index. Use `lexa index .` to refresh a project's graph.

Release installers verify the selected archive against the release's `SHA256SUMS` file before extraction. If checksum verification fails, installation stops without replacing the existing binary.

## Troubleshooting

If `lexa --version` is unavailable after install, verify that the install directory is on `PATH`.

If the installer cannot determine the right binary, build from source with `cargo install --path /path/to/lexa --force`.
