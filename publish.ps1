# Cross-platform AOT publishing script for HtmlSlotCompiler
# Publishes to all major architectures and operating systems

$ErrorActionPreference = "Stop"

# Define target platforms
$platforms = @(
    @{ rid = "win-x64"; os = "Windows"; arch = "x64" },
    @{ rid = "win-arm64"; os = "Windows"; arch = "ARM64" },
    @{ rid = "linux-x64"; os = "Linux"; arch = "x64" },
    @{ rid = "linux-arm64"; os = "Linux"; arch = "ARM64" },
    @{ rid = "osx-x64"; os = "macOS"; arch = "x64" },
    @{ rid = "osx-arm64"; os = "macOS"; arch = "ARM64" }
)

# Create publish directory
$publishDir = "publish"
if (!(Test-Path $publishDir)) {
    New-Item -ItemType Directory -Path $publishDir | Out-Null
}

Write-Host "Building HtmlSlotCompiler for all platforms..." -ForegroundColor Cyan
Write-Host ""

foreach ($platform in $platforms) {
    $rid = $platform.rid
    $os = $platform.os
    $arch = $platform.arch
    $outputDir = Join-Path $publishDir "$rid"
    $exeName = if ($rid.StartsWith("win")) { "SiteCompiler.exe" } else { "SiteCompiler" }

    Write-Host "[$os/$arch] Publishing for runtime identifier: $rid" -ForegroundColor Yellow

    try {
        dotnet publish -c Release -r $rid `
            -o $outputDir 2>&1 | Out-Null

        $exePath = Join-Path $outputDir $exeName
        if (Test-Path $exePath) {
            $size = (Get-Item $exePath).Length / 1MB
            Write-Host "  ✔ Success: $exePath ($('{0:F2}' -f $size) MB)" -ForegroundColor Green
        } else {
            Write-Host "  ✗ Error: Executable not found at $exePath" -ForegroundColor Red
        }
    } catch {
        Write-Host "  ✗ Error: $_" -ForegroundColor Red
    }

    Write-Host ""
}

Write-Host "Publishing complete!" -ForegroundColor Cyan
Write-Host "Binaries available in: $publishDir" -ForegroundColor Green
