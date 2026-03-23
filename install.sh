#!/bin/bash
set -e

OWNER="nnar1o"
REPO="kg"
BINARIES=("kg" "kg-mcp" "kg-tui")

install_dir="${KG_INSTALL_DIR:-$HOME/.local/bin}"

echo "Installing kg to $install_dir..."

mkdir -p "$install_dir"

for bin in "${BINARIES[@]}"; do
    url="https://github.com/$OWNER/$REPO/releases/latest/download/$bin"
    echo "Downloading $bin..."
    curl -sSL "$url" -o "$install_dir/$bin"
    chmod +x "$install_dir/$bin"
done

echo "Done! Add $install_dir to your PATH:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
