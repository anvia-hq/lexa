# lexa-index

`lexa-index` installs the precompiled [Lexa](https://github.com/anvia-hq/lexa)
Rust executable for the current platform. The npm package is named
`lexa-index`; the installed command remains `lexa`.

```bash
npm install -g lexa-index
lexa --version
```

Or run it without a global installation:

```bash
npx lexa-index --version
```

No Rust compiler is required. npm selects a native package for macOS ARM64,
macOS x64, Linux x64, or Windows x64. The existing curl and PowerShell
installers remain supported.
