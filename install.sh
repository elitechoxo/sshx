#!/usr/bin/env sh
# sshx client install script
# Usage: curl -fsSL https://teamxpirates.qzz.io/install.sh | sh

set -e

REPO="https://github.com/elitechoxo/SSHX-tunnel"   # update this when you publish
BIN_DIR="${HOME}/.local/bin"
BIN="${BIN_DIR}/sshx"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

case "$OS" in
  linux)  TARGET="${ARCH}-unknown-linux-musl" ;;
  darwin) TARGET="${ARCH}-apple-darwin" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

LATEST=$(curl -fsSL "${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "\(.*\)".*/\1/')
URL="${REPO}/releases/download/${LATEST}/sshx-${TARGET}"

echo "Installing sshx ${LATEST} for ${TARGET}…"
mkdir -p "$BIN_DIR"
curl -fsSL "$URL" -o "$BIN"
chmod +x "$BIN"

echo ""
echo "✓ sshx installed to $BIN"
echo ""
echo "  Make sure $BIN_DIR is in your PATH:"
echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "  Usage:"
echo "    sshx -s myapp -p 3000"
echo "    sshx -s myssh -p 22 --tcp"
