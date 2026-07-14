# Install Lexa

## npm

The public npm package is named `lexa-index`; the installed command remains
`lexa`.

```bash
npm install -g lexa-index
lexa --version
```

It can also be run directly with npx:

```bash
npx lexa-index --version
```

npm selects a precompiled Rust executable for macOS ARM64, macOS x64, or Linux
x64. Rust is not compiled on the user's machine, and no binary is downloaded
by an install or postinstall script. Windows remains supported through the
PowerShell installer below.

The curl and PowerShell installers below remain fully supported.

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
