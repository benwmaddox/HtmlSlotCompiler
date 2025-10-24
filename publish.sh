#!/bin/bash
# Cross-platform AOT publishing script for HtmlSlotCompiler
# Publishes to all major architectures and operating systems

set -e

# On Windows with Visual Studio Build Tools, ensure vswhere is in PATH
if [ -f "/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe" ]; then
    export PATH="/c/Program Files (x86)/Microsoft Visual Studio/Installer:$PATH"
elif [ -f "/opt/hostedtoolcache/windows/msvc-toolchain" ]; then
    # CI environments may have it elsewhere
    export PATH="/opt/hostedtoolcache/windows/msvc-toolchain:$PATH"
fi

# Define target platforms
declare -a PLATFORMS=(
    "win-x64:Windows:x64"
    "win-arm64:Windows:ARM64"
    "linux-x64:Linux:x64"
    "linux-arm64:Linux:ARM64"
    "osx-x64:macOS:x64"
    "osx-arm64:macOS:ARM64"
)

# Create publish directory
PUBLISH_DIR="publish"
mkdir -p "$PUBLISH_DIR"

echo "Building HtmlSlotCompiler for all platforms..."
echo ""

for platform in "${PLATFORMS[@]}"; do
    IFS=':' read -r RID OS ARCH <<< "$platform"
    OUTPUT_DIR="$PUBLISH_DIR/$RID"
    EXE_NAME=$([ "$RID" = "${RID#win}" ] && echo "SiteCompiler" || echo "SiteCompiler.exe")

    echo "[$OS/$ARCH] Publishing for runtime identifier: $RID"

    if dotnet publish -c Release -r "$RID" \
        -o "$OUTPUT_DIR" 2>&1 > /dev/null; then

        EXE_PATH="$OUTPUT_DIR/$EXE_NAME"
        if [ -f "$EXE_PATH" ]; then
            SIZE=$(ls -lh "$EXE_PATH" | awk '{print $5}')
            echo "  [OK] Success: $EXE_PATH ($SIZE)"
        else
            echo "  [FAIL] Error: Executable not found at $EXE_PATH"
        fi
    else
        echo "  [FAIL] Error: Build failed for $RID"
    fi

    echo ""
done

echo "Publishing complete!"
echo "Binaries available in: $PUBLISH_DIR"
