#!/bin/sh
# vapx installer for Linux and macOS.
#
#   curl -fsSL https://vapx.29a.se/install | sh
#
# Read it first if you like (it is short):
#   curl -fsSL https://vapx.29a.se/install | less
#
# What it does: picks the release binary for this OS and CPU, downloads it and
# SHA256SUMS from the GitHub release, verifies the checksum, and puts vapx in
# ~/.local/bin (no sudo). Running it again updates to the latest release.
#
#   VAPX_VERSION=0.26.0 sh install.sh    install a specific version
#   VAPX_INSTALL_DIR=/opt/bin sh install.sh
#   sh install.sh --uninstall            remove vapx from the install dir
#
# The man page, shell completions and cameras.yaml are not touched.

set -eu

REPO="Hex29A/vapx"
INSTALL_DIR="${VAPX_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${VAPX_VERSION:-}"

say() { printf '%s\n' "$*"; }
die() { printf 'vapx install: %s\n' "$*" >&2; exit 1; }

case "${1:-}" in
  --uninstall)
    if [ -e "$INSTALL_DIR/vapx" ]; then
      rm -f "$INSTALL_DIR/vapx"
      say "Removed $INSTALL_DIR/vapx."
      say "Your cameras.yaml (~/.config/vapx/) is left in place."
    else
      say "No vapx in $INSTALL_DIR, nothing to remove."
    fi
    exit 0
    ;;
  -h|--help)
    sed -n '2,17p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//' || true
    exit 0
    ;;
  "") ;;
  *) die "unknown option: $1 (try --help)" ;;
esac

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Linux)
    case "$arch" in
      x86_64|amd64)   target="x86_64-unknown-linux-musl" ;;
      aarch64|arm64)  target="aarch64-unknown-linux-musl" ;;
      armv7l|armv7*)  target="armv7-unknown-linux-musleabihf" ;;
      *) die "no prebuilt binary for Linux $arch; build from source with cargo install vapx" ;;
    esac
    ;;
  Darwin)
    case "$arch" in
      arm64|aarch64)  target="aarch64-apple-darwin" ;;
      x86_64)         target="x86_64-apple-darwin" ;;
      *) die "no prebuilt binary for macOS $arch" ;;
    esac
    ;;
  *) die "unsupported OS $os; on Windows use install.ps1" ;;
esac

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q -O "$2" "$1"; }
else
  die "needs curl or wget"
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "needs sha256sum or shasum to verify the download"
fi

if [ -n "$VERSION" ]; then
  VERSION="${VERSION#v}"
  base="https://github.com/$REPO/releases/download/v$VERSION"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

asset="vapx-$target"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading $asset${VERSION:+ v$VERSION}..."
fetch "$base/$asset" "$tmp/vapx" || die "download failed: $base/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" \
  || die "could not fetch SHA256SUMS (releases before v0.27.0 have none)"

want=$(awk -v f="$asset" '$2 == f || $2 == "*"f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$want" ] || die "$asset is not listed in SHA256SUMS"
got=$(sha256 "$tmp/vapx")
[ "$want" = "$got" ] || die "checksum mismatch for $asset (expected $want, got $got)"

chmod +x "$tmp/vapx"
mkdir -p "$INSTALL_DIR"
old=""
[ -x "$INSTALL_DIR/vapx" ] && old=$("$INSTALL_DIR/vapx" --version 2>/dev/null || true)
mv "$tmp/vapx" "$INSTALL_DIR/vapx"
new=$("$INSTALL_DIR/vapx" --version 2>/dev/null || echo vapx)

if [ -n "$old" ] && [ "$old" != "$new" ]; then
  say "Updated $old -> $new in $INSTALL_DIR"
elif [ -n "$old" ]; then
  say "$new in $INSTALL_DIR is already current (checksum verified)"
else
  say "Installed $new in $INSTALL_DIR (checksum verified)"
fi

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    say ""
    say "$INSTALL_DIR is not on your PATH. Add it, e.g.:"
    say "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile"
    ;;
esac

say ""
say "Next: vapx info <ip> -u root -p <password>   (or vapx --help)"
