#!/bin/sh
set -e

REPO="dr-alberto/rewindr"
BINARY="rewindr"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS
OS=$(uname -s)
case "$OS" in
  Linux)  OS_NAME="linux" ;;
  Darwin) OS_NAME="macos" ;;
  *)
    printf 'Unsupported OS: %s\n' "$OS" >&2
    exit 1
    ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64)  ARCH_NAME="amd64" ;;
  aarch64|arm64) ARCH_NAME="arm64" ;;
  *)
    printf 'Unsupported architecture: %s\n' "$ARCH" >&2
    exit 1
    ;;
esac

# Fetch latest release tag
VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$VERSION" ]; then
  printf 'Could not determine latest version.\n' >&2
  exit 1
fi

ASSET="${BINARY}-${OS_NAME}-${ARCH_NAME}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"

printf 'Installing %s %s (%s/%s)...\n' "$BINARY" "$VERSION" "$OS_NAME" "$ARCH_NAME"

# Download and extract
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "$URL" | tar xz -C "$TMP"

# Install — sudo only if the target dir isn't writable
if [ -w "$INSTALL_DIR" ]; then
  install -m 755 "${TMP}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
else
  sudo install -m 755 "${TMP}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
fi

printf '%s installed to %s/%s\n' "$BINARY" "$INSTALL_DIR" "$BINARY"
