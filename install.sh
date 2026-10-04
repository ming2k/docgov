#!/bin/sh
# install.sh — Universal installer for docgov (Document Governance)
# Usage: curl -fsSL https://raw.githubusercontent.com/ming2k/docgov/main/install.sh | sh
set -e

REPO="ming2k/docgov"
BIN_NAME="docgov"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux*)     OS="unknown-linux-musl" ;;
    Darwin*)    OS="apple-darwin" ;;
    *)          echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

# Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)   ARCH="x86_64" ;;
    aarch64|arm64)  ARCH="aarch64" ;;
    *)              echo "Unsupported Architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"

# Determine version
VERSION="${DOCGOV_VERSION:-${DOG_VERSION:-latest}}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

download_file() {
    url="$1"
    dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    else
        echo "Error: curl or wget is required" >&2
        return 1
    fi
}

ARCHIVE="${TMP_DIR}/docgov.tar.gz"
DOWNLOADED=0

if [ "$VERSION" = "latest" ]; then
    CANDIDATE_URLS="
https://github.com/${REPO}/releases/latest/download/docgov-${TARGET}.tar.gz
https://github.com/${REPO}/releases/latest/download/dog-${TARGET}.tar.gz
"
else
    # Normalize tag name: e.g. 0.0.1 -> v0.0.1
    TAG="$VERSION"
    case "$TAG" in
        v*) ;;
        *) TAG="v$TAG" ;;
    esac
    CANDIDATE_URLS="
https://github.com/${REPO}/releases/download/${TAG}/docgov-${TARGET}.tar.gz
https://github.com/${REPO}/releases/download/${TAG}/dog-${TARGET}.tar.gz
https://github.com/${REPO}/releases/download/${VERSION}/docgov-${TARGET}.tar.gz
https://github.com/${REPO}/releases/download/${VERSION}/dog-${TARGET}.tar.gz
"
fi

for url in $CANDIDATE_URLS; do
    if download_file "$url" "$ARCHIVE" 2>/dev/null; then
        DOWNLOADED=1
        break
    fi
done

if [ "$DOWNLOADED" -eq 0 ]; then
    echo "Error: Failed to download release binary for ${TARGET} (version: ${VERSION})" >&2
    exit 1
fi

tar -xzf "$ARCHIVE" -C "$TMP_DIR"

# Determine install directory
if [ -n "$DOCGOV_INSTALL_DIR" ]; then
    INSTALL_DIR="$DOCGOV_INSTALL_DIR"
elif [ -n "$DOG_INSTALL_DIR" ]; then
    INSTALL_DIR="$DOG_INSTALL_DIR"
elif [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
elif [ -w "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
    INSTALL_DIR="$HOME/.local/bin"
else
    INSTALL_DIR="/usr/local/bin"
fi

echo "==> Installing ${BIN_NAME} (${TARGET}) to ${INSTALL_DIR}..."

# Locate extracted binary
EXTRACTED_BIN=""
if [ -f "${TMP_DIR}/docgov" ]; then
    EXTRACTED_BIN="${TMP_DIR}/docgov"
elif [ -f "${TMP_DIR}/dog" ]; then
    EXTRACTED_BIN="${TMP_DIR}/dog"
else
    echo "Error: No docgov executable found in archive" >&2
    exit 1
fi

install_bin() {
    src="$1"
    target_name="$2"
    if [ -w "$INSTALL_DIR" ]; then
        cp "$src" "${INSTALL_DIR}/${target_name}"
        chmod +x "${INSTALL_DIR}/${target_name}"
    else
        sudo cp "$src" "${INSTALL_DIR}/${target_name}"
        sudo chmod +x "${INSTALL_DIR}/${target_name}"
    fi
}

install_bin "$EXTRACTED_BIN" "docgov"
# Provide alias for dog if available
if [ -f "${TMP_DIR}/dog" ]; then
    install_bin "${TMP_DIR}/dog" "dog"
else
    install_bin "$EXTRACTED_BIN" "dog"
fi

echo "==> Successfully installed ${BIN_NAME} to ${INSTALL_DIR}"

case ":$PATH:" in
    *:"$INSTALL_DIR":*) ;;
    *)
        echo ""
        echo "WARNING: ${INSTALL_DIR} is not in your \$PATH."
        echo "Add it by running:"
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
        echo ""
        ;;
esac

echo "Run 'docgov --help' to get started."
