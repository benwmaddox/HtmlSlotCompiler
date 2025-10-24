#!/bin/bash
# Cross-platform AOT publishing script for HtmlSlotCompiler
# Publishes to all major architectures and operating systems

set -e

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

echo -e "\033[36mBuilding HtmlSlotCompiler for all platforms...\033[0m"
echo ""

for platform in "${PLATFORMS[@]}"; do
    IFS=':' read -r RID OS ARCH <<< "$platform"
    OUTPUT_DIR="$PUBLISH_DIR/$RID"
    EXE_NAME=$([ "$RID" = "${RID#win}" ] && echo "SiteCompiler" || echo "SiteCompiler.exe")

    echo -e "\033[33m[$OS/$ARCH] Publishing for runtime identifier: $RID\033[0m"

    if dotnet publish -c Release -r "$RID" \
        -p:PublishAot=true \
        -p:PublishTrimmed=true \
        -p:InvariantGlobalization=true \
        -p:SelfContained=true \
        -o "$OUTPUT_DIR" 2>&1 > /dev/null; then

        EXE_PATH="$OUTPUT_DIR/$EXE_NAME"
        if [ -f "$EXE_PATH" ]; then
            SIZE=$(ls -lh "$EXE_PATH" | awk '{print $5}')
            echo -e "  \033[32m✔ Success: $EXE_PATH ($SIZE)\033[0m"
        else
            echo -e "  \033[31m✗ Error: Executable not found at $EXE_PATH\033[0m"
        fi
    else
        echo -e "  \033[31m✗ Error: Build failed for $RID\033[0m"
    fi

    echo ""
done

echo -e "\033[36mPublishing complete!\033[0m"
echo -e "\033[32mBinaries available in: $PUBLISH_DIR\033[0m"
