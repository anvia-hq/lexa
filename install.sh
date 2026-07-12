#!/bin/sh
set -eu

repo="${LEXA_REPO:-anvia-hq/lexa}"
version="${1:-${LEXA_VERSION:-latest}}"
install_dir="${LEXA_INSTALL_DIR:-$HOME/.local/bin}"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'error: required command not found: %s\n' "$1" >&2
    exit 1
  fi
}

need_cmd curl
need_cmd tar
need_cmd uname
need_cmd sed
need_cmd mktemp

os="$(uname -s)"
arch="$(uname -m)"

case "$os:$arch" in
  Darwin:arm64 | Darwin:aarch64)
    platform="macos-apple-silicon"
    ;;
  Darwin:x86_64 | Darwin:amd64)
    platform="macos-intel"
    ;;
  Linux:x86_64 | Linux:amd64)
    platform="linux-x86_64"
    ;;
  *)
    printf 'error: unsupported platform: %s %s\n' "$os" "$arch" >&2
    printf 'supported platforms: macOS Apple Silicon, macOS Intel, Linux x86_64\n' >&2
    exit 1
    ;;
esac

if [ "$version" = "latest" ]; then
  tag="$(
    curl -fsSL "https://api.github.com/repos/$repo/releases/latest" |
      sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' |
      sed -n '1p'
  )"
  if [ -z "$tag" ]; then
    printf 'error: could not determine latest release for %s\n' "$repo" >&2
    exit 1
  fi
else
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
fi

asset_version="${tag#v}"
archive="lexa-${platform}-${asset_version}.tar.gz"
url="https://github.com/$repo/releases/download/$tag/$archive"
tmp_dir="$(mktemp -d)"
install_tmp=""

cleanup() {
  rm -rf "$tmp_dir"
  if [ -n "$install_tmp" ]; then
    rm -f "$install_tmp"
  fi
}
trap cleanup EXIT INT TERM

printf 'Downloading %s...\n' "$url"
curl -fL --retry 3 --retry-delay 2 -o "$tmp_dir/$archive" "$url"
checksum_url="https://github.com/$repo/releases/download/$tag/SHA256SUMS"
curl -fL --retry 3 --retry-delay 2 -o "$tmp_dir/SHA256SUMS" "$checksum_url"
expected_checksum="$(
  sed -n "s/^\\([[:xdigit:]]*\\)[[:space:]][[:space:]]*$archive$/\\1/p" "$tmp_dir/SHA256SUMS" |
    sed -n '1p'
)"
if [ -z "$expected_checksum" ]; then
  printf 'error: checksum file did not contain %s\n' "$archive" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual_checksum="$(sha256sum "$tmp_dir/$archive" | sed 's/[[:space:]].*//')"
elif command -v shasum >/dev/null 2>&1; then
  actual_checksum="$(shasum -a 256 "$tmp_dir/$archive" | sed 's/[[:space:]].*//')"
else
  printf 'error: sha256sum or shasum is required to verify the download\n' >&2
  exit 1
fi
if [ "$actual_checksum" != "$expected_checksum" ]; then
  printf 'error: checksum mismatch for %s\n' "$archive" >&2
  exit 1
fi

tar -xzf "$tmp_dir/$archive" -C "$tmp_dir"
binary="$tmp_dir/lexa-${platform}-${asset_version}/lexa"

if [ ! -f "$binary" ]; then
  printf 'error: archive did not contain expected binary: lexa\n' >&2
  exit 1
fi

mkdir -p "$install_dir"
install_tmp="$(mktemp "$install_dir/.lexa.XXXXXX")"
cp "$binary" "$install_tmp"
chmod 755 "$install_tmp"

"$install_tmp" --help >/dev/null
mv -f "$install_tmp" "$install_dir/lexa"
install_tmp=""

printf 'Installed lexa %s to %s/lexa\n' "$tag" "$install_dir"

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    printf 'Add this directory to PATH to run lexa from anywhere:\n'
    printf '  export PATH="%s:$PATH"\n' "$install_dir"
    ;;
esac

"$install_dir/lexa" --help >/dev/null
printf 'lexa is ready.\n'
