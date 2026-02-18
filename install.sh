#!/usr/bin/env sh
set -e

REPO="elitechoxo/sshx"
BIN_DIR="/usr/local/bin"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64)        ARCH="x86_64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

case "$OS" in
  linux)  FILE="sshx-linux-${ARCH}" ;;
  darwin) FILE="sshx-macos-${ARCH}" ;;
  *) echo "Download from https://github.com/${REPO}/releases"; exit 1 ;;
esac

LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "\(.*\)".*/\1/')
URL="https://github.com/${REPO}/releases/download/${LATEST}/${FILE}"

echo "Installing sshx ${LATEST}..."
curl -fsSL "$URL" -o /tmp/sshx
chmod +x /tmp/sshx
sudo mv /tmp/sshx "$BIN_DIR/sshx"

echo "✓ Done! Run: sshx -s myapp -p 3000"
echo "  Make sure $BIN_DIR is in your PATH:"
echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "  Usage:"
echo "    sshx -s myapp -p 3000"
echo "    sshx -s myssh -p 22 --tcp"
