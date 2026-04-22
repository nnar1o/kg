#!/bin/sh
set -eu

OWNER="nnar1o"
REPO="kg"
BINARIES="kg kg-mcp kg-tui"

install_dir="${KG_INSTALL_DIR:-$HOME/.local/bin}"

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os/$arch" in
        Linux/x86_64)
            printf '%s\n' "x86_64-unknown-linux-gnu"
            ;;
        Darwin/x86_64)
            printf '%s\n' "x86_64-apple-darwin"
            ;;
        Darwin/arm64|Darwin/aarch64)
            printf '%s\n' "aarch64-apple-darwin"
            ;;
        *)
            return 1
            ;;
    esac
}

target="$(detect_target)" || {
    echo "No prebuilt binaries for $(uname -s)/$(uname -m)." >&2
    exit 1
}

echo "Installing kg ($target) to $install_dir..."

mkdir -p "$install_dir"

for bin in $BINARIES; do
    asset="$bin-$target"
    url="https://github.com/$OWNER/$REPO/releases/download/v0.2.16/$asset"
    echo "Downloading $bin..."
    curl -fsSL "$url" -o "$install_dir/$bin"
    chmod +x "$install_dir/$bin"
done

echo "Done! Add $install_dir to your PATH:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
