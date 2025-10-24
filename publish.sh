#!/bin/bash
# AOT publishing script for HtmlSlotCompiler (Windows x64)
# Produces a single native executable

set -e

# On Windows with Visual Studio Build Tools, ensure vswhere is in PATH
if [ -f "/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe" ]; then
    export PATH="/c/Program Files (x86)/Microsoft Visual Studio/Installer:$PATH"
fi

# Publish configuration
OUTPUT_DIR="publish"
EXE_NAME="SiteCompiler.exe"
BIN_PATH="bin/Release/net8.0/win-x64/publish/$EXE_NAME"

echo "Building $EXE_NAME (Windows x64 AOT)..."

if dotnet publish -c Release 2>&1 > /dev/null; then
    # Copy executable to publish folder
    mkdir -p "$OUTPUT_DIR"
    cp "$BIN_PATH" "$OUTPUT_DIR/"

    EXE_PATH="$OUTPUT_DIR/$EXE_NAME"
    if [ -f "$EXE_PATH" ]; then
        SIZE=$(ls -lh "$EXE_PATH" | awk '{print $5}')
        echo "[OK] Built: $EXE_PATH ($SIZE)"
        echo ""
        echo "Ready to use:"
        echo "  ./$EXE_NAME <source-dir> <output-dir>"
    else
        echo "[FAIL] Executable not found at $EXE_PATH" >&2
        exit 1
    fi
else
    echo "[FAIL] Build failed" >&2
    exit 1
fi
