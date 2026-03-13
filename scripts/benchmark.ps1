param(
    [int]$PageCount = 100,
    [int]$BuildIterations = 5,
    [int]$WatchIterations = 3,
    [int]$WatchTimeoutSeconds = 15,
    [string]$CompilerPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Get-CompilerPath {
    param(
        [string]$RepoRoot,
        [string]$RequestedPath
    )

    if ($RequestedPath) {
        $resolved = Resolve-Path $RequestedPath -ErrorAction Stop
        return $resolved.Path
    }

    $windowsPath = Join-Path $RepoRoot "rust\target\release\site-compiler.exe"
    if (Test-Path $windowsPath) {
        return $windowsPath
    }

    $unixPath = Join-Path $RepoRoot "rust\target\release\site-compiler"
    if (Test-Path $unixPath) {
        return $unixPath
    }

    throw "Compiler binary not found. Build it first with: cargo build --release"
}

function Write-Utf8File {
    param(
        [string]$Path,
        [string]$Content
    )

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Content, $utf8NoBom)
}

function New-BenchmarkFixture {
    param(
        [string]$SourceDir,
        [string]$OutputDir,
        [int]$TotalPages
    )

    $benchRoot = Split-Path $SourceDir -Parent
    if (Test-Path $benchRoot) {
        Remove-Item $benchRoot -Recurse -Force
    }

    New-Item -ItemType Directory -Path $SourceDir | Out-Null
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $SourceDir "css") | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $SourceDir "js") | Out-Null

    $layout = @"
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title slot="title"></title>
    <meta slot="description" slot-mode="attr:content" content="" />
    <link rel="stylesheet" href="css/site.css" />
  </head>
  <body>
    <header slot="header"></header>
    <main slot="content"></main>
    <footer slot="footer"></footer>
    <script src="js/site.js"></script>
  </body>
</html>
"@
    Write-Utf8File -Path (Join-Path $SourceDir "_layout.html") -Content $layout

    $css = @"
body {
  font-family: sans-serif;
  margin: 0;
  padding: 2rem;
}

.page-card {
  border: 1px solid #ccc;
  padding: 1rem;
}
"@
    Write-Utf8File -Path (Join-Path $SourceDir "css\site.css") -Content $css

    $js = @"
document.documentElement.dataset.benchmark = "ready";
"@
    Write-Utf8File -Path (Join-Path $SourceDir "js\site.js") -Content $js

    for ($i = 1; $i -le $TotalPages; $i++) {
        $slug = if ($i -eq 1) { "index" } else { "page-{0:D3}" -f $i }
        $title = "Benchmark Page {0:D3}" -f $i
        $description = "Synthetic benchmark page {0:D3}" -f $i
        $body = @"
<title for-slot="title">$title</title>
<meta for-slot="description" content="$description" />
<section for-slot="header">
  <h1>$title</h1>
</section>
<section for-slot="content">
  <article class="page-card">
    <p>This is generated benchmark page {0:D3}.</p>
  </article>
</section>
<section for-slot="footer">
  <p>Footer {0:D3}</p>
</section>
"@ -f $i

        Write-Utf8File -Path (Join-Path $SourceDir "$slug.html") -Content $body
    }
}

function Get-BuildCompleteCount {
    param([string]$LogPath)

    if (!(Test-Path $LogPath)) {
        return 0
    }

    return ([regex]::Matches((Get-Content -Raw $LogPath), "\[Build\] Complete in \d+ ms\.")).Count
}

function Wait-ForWatchReady {
    param(
        [string]$LogPath,
        [int]$TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path $LogPath) {
            $log = Get-Content -Raw $LogPath
            if ($log -match "\[Watch\] Watching for changes" -and $log -match "\[Build\] Complete in \d+ ms\.") {
                return
            }
        }
        Start-Sleep -Milliseconds 100
    }

    throw "Timed out waiting for watch mode to start."
}

function Wait-ForNextBuild {
    param(
        [string]$LogPath,
        [int]$PreviousCount,
        [int]$TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if ((Get-BuildCompleteCount -LogPath $LogPath) -gt $PreviousCount) {
            return
        }
        Start-Sleep -Milliseconds 50
    }

    throw "Timed out waiting for watch rebuild."
}

function Get-Stats {
    param([double[]]$Values)

    return [pscustomobject]@{
        Min     = [Math]::Round(($Values | Measure-Object -Minimum).Minimum, 2)
        Max     = [Math]::Round(($Values | Measure-Object -Maximum).Maximum, 2)
        Average = [Math]::Round(($Values | Measure-Object -Average).Average, 2)
    }
}

$repoRoot = Get-RepoRoot
$compiler = Get-CompilerPath -RepoRoot $repoRoot -RequestedPath $CompilerPath
$benchmarkRoot = Join-Path $repoRoot "benchmarks\generated\100-pages"
$sourceDir = Join-Path $benchmarkRoot "src"
$outputDir = Join-Path $benchmarkRoot "dist"
$resultsDir = Join-Path $repoRoot "benchmarks\results"

New-Item -ItemType Directory -Path $resultsDir -Force | Out-Null
New-BenchmarkFixture -SourceDir $sourceDir -OutputDir $outputDir -TotalPages $PageCount

$buildTimings = New-Object System.Collections.Generic.List[double]
for ($i = 1; $i -le $BuildIterations; $i++) {
    if (Test-Path $outputDir) {
        Remove-Item $outputDir -Recurse -Force
    }

    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    & $compiler $sourceDir $outputDir | Out-Null
    $exitCode = $LASTEXITCODE
    $timer.Stop()

    if ($exitCode -ne 0) {
        throw "Compiler exited with code $exitCode during build iteration $i."
    }

    $buildTimings.Add([Math]::Round($timer.Elapsed.TotalMilliseconds, 2))
}

$htmlCount = (Get-ChildItem $outputDir -Filter *.html).Count
$cssExists = Test-Path (Join-Path $outputDir "css\site.css")
$jsExists = Test-Path (Join-Path $outputDir "js\site.js")
if ($htmlCount -ne $PageCount -or -not $cssExists -or -not $jsExists) {
    throw "Unexpected build output. HTML count: $htmlCount, CSS: $cssExists, JS: $jsExists"
}

$watchStdOut = Join-Path $resultsDir "watch-benchmark.stdout.log"
$watchStdErr = Join-Path $resultsDir "watch-benchmark.stderr.log"
if (Test-Path $watchStdOut) { Remove-Item $watchStdOut -Force }
if (Test-Path $watchStdErr) { Remove-Item $watchStdErr -Force }

$watchProcess = Start-Process -FilePath $compiler `
    -ArgumentList @($sourceDir, $outputDir, "--watch") `
    -RedirectStandardOutput $watchStdOut `
    -RedirectStandardError $watchStdErr `
    -PassThru

$watchTimings = New-Object System.Collections.Generic.List[double]

try {
    Wait-ForWatchReady -LogPath $watchStdOut -TimeoutSeconds $WatchTimeoutSeconds

    for ($i = 1; $i -le $WatchIterations; $i++) {
        $targetPage = if ($i -eq 1) { "index.html" } else { "page-{0:D3}.html" -f ($i + 1) }
        $targetPath = Join-Path $sourceDir $targetPage
        $baseline = Get-BuildCompleteCount -LogPath $watchStdOut
        $content = Get-Content -Raw $targetPath
        $updatedContent = $content -replace "generated benchmark page \d{3}\.", ("generated benchmark page {0:D3}. Update {1}." -f ($i + 1), $i)

        $timer = [System.Diagnostics.Stopwatch]::StartNew()
        Write-Utf8File -Path $targetPath -Content $updatedContent
        Wait-ForNextBuild -LogPath $watchStdOut -PreviousCount $baseline -TimeoutSeconds $WatchTimeoutSeconds
        $timer.Stop()

        $watchTimings.Add([Math]::Round($timer.Elapsed.TotalMilliseconds, 2))
    }
}
finally {
    if ($watchProcess -and -not $watchProcess.HasExited) {
        Stop-Process -Id $watchProcess.Id -Force
        $watchProcess.WaitForExit()
    }
}

$summary = [pscustomobject]@{
    Compiler      = $compiler
    SourceDir     = $sourceDir
    OutputDir     = $outputDir
    Pages         = $PageCount
    BuildRuns     = $BuildIterations
    BuildStatsMs  = Get-Stats -Values $buildTimings.ToArray()
    WatchRuns     = $WatchIterations
    WatchStatsMs  = Get-Stats -Values $watchTimings.ToArray()
    HtmlOutputs   = $htmlCount
    CssCopied     = $cssExists
    JsCopied      = $jsExists
    WatchStdOut   = $watchStdOut
    WatchStdErr   = $watchStdErr
}

$summaryPath = Join-Path $resultsDir "benchmark-summary.json"
$summary | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $summaryPath

Write-Host ""
Write-Host "Benchmark complete"
Write-Host "Compiler: $compiler"
Write-Host "Pages: $PageCount"
Write-Host "Build ms  : min $($summary.BuildStatsMs.Min), avg $($summary.BuildStatsMs.Average), max $($summary.BuildStatsMs.Max)"
Write-Host "Watch ms  : min $($summary.WatchStatsMs.Min), avg $($summary.WatchStatsMs.Average), max $($summary.WatchStatsMs.Max)"
Write-Host "Output    : $htmlCount HTML, css copied=$cssExists, js copied=$jsExists"
Write-Host "Summary   : $summaryPath"
