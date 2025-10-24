# AOT publishing script for HtmlSlotCompiler (Windows x64)
# Produces a single native executable

$ErrorActionPreference = "Stop"

# Ensure vswhere is in PATH for Visual Studio Build Tools detection
$vsInstallerPath = "C:\Program Files (x86)\Microsoft Visual Studio\Installer"
if (!(Test-Path $vsInstallerPath)) {
    Write-Host "Error: Visual Studio Build Tools not found." -ForegroundColor Red
    Write-Host "Install from: https://visualstudio.microsoft.com/downloads/" -ForegroundColor Yellow
    exit 1
}
$env:PATH = "$vsInstallerPath;$env:PATH"

# Publish configuration
$outputDir = "publish"
$exeName = "SiteCompiler.exe"
$binPath = "bin\Release\net8.0\win-x64\publish\$exeName"

Write-Host "Building $exeName (Windows x64 AOT)..." -ForegroundColor Cyan

try {
    # Build with dotnet publish
    dotnet publish -c Release | Out-Null

    # Copy executable to publish folder
    if (!(Test-Path $outputDir)) {
        New-Item -ItemType Directory -Path $outputDir | Out-Null
    }
    Copy-Item $binPath -Destination $outputDir -Force

    $exePath = Join-Path $outputDir $exeName
    if (Test-Path $exePath) {
        $size = (Get-Item $exePath).Length / 1MB
        Write-Host "[OK] Built: $exePath ($([Math]::Round($size, 2)) MB)" -ForegroundColor Green
        Write-Host ""
        Write-Host "Ready to use:" -ForegroundColor Cyan
        Write-Host "  .\$exeName <source-dir> <output-dir>" -ForegroundColor White
    } else {
        Write-Host "[FAIL] Executable not found at $exePath" -ForegroundColor Red
        exit 1
    }
} catch {
    Write-Host "[FAIL] Build error: $_" -ForegroundColor Red
    exit 1
}
