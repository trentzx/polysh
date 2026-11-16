#!/bin/sh
# Install polysh on macOS or Linux:
#   curl -fsSL https://raw.githubusercontent.com/trentzx/polysh/main/install.sh | sh
#
# Set POLYSH_INSTALL_DIR to choose where the binary goes (default: ~/.local/bin).
set -eu

REPO="trentzx/polysh"
BIN_DIR="${POLYSH_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
  echo "polysh: $1" >&2
  exit 1
}

os=$(uname -s)
arch=$(uname -m)
case "$os/$arch" in
  Darwin/arm64) target="aarch64-apple-darwin" ;;
  Darwin/x86_64) target="x86_64-apple-darwin" ;;
  Linux/x86_64) target="x86_64-unknown-linux-musl" ;;
  *) fail "no prebuilt binary for $os $arch yet. Try: cargo install --git https://github.com/$REPO" ;;
esac

# Follow the releases/latest redirect instead of the API, which is rate-limited.
tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")
tag=${tag##*/}
case "$tag" in v*) ;; *) tag="" ;; esac
[ -n "$tag" ] || fail "couldn't find the latest release"

name="polysh-$tag-$target"
url="https://github.com/$REPO/releases/download/$tag/$name.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading polysh $tag for $target..."
curl -fsSL "$url" -o "$tmp/$name.tar.gz"
curl -fsSL "$url.sha256" -o "$tmp/$name.tar.gz.sha256"

expected=$(cut -d ' ' -f 1 < "$tmp/$name.tar.gz.sha256")
if command -v sha256sum > /dev/null; then
  actual=$(sha256sum "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
else
  actual=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
fi
[ "$expected" = "$actual" ] || fail "checksum mismatch; download may be corrupted"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$BIN_DIR"
cp "$tmp/$name/polysh" "$BIN_DIR/polysh"
chmod 755 "$BIN_DIR/polysh"

echo "Installed polysh $tag to $BIN_DIR/polysh"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "Run: polysh" ;;
  *) echo "Add it to your PATH, then run polysh:"
     echo "  export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
