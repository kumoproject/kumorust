param(
    [string]$Target = "aarch64-pc-windows-msvc",
    [string]$Runtime = "win-arm64",
    [string]$Version = "",
    [string]$OutputDir = "target\velopack-releases"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

if ([string]::IsNullOrWhiteSpace($Version)) {
    $cargoToml = Get-Content (Join-Path $repoRoot "Cargo.toml") -Raw
    $versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $versionMatch.Success) {
        throw "Could not read the package version from Cargo.toml."
    }
    $Version = $versionMatch.Groups[1].Value
}

Push-Location $repoRoot
try {
    & cargo build --release --locked --target $Target
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }

    $releaseDir = Join-Path $repoRoot "target\$Target\release"
    $stageDir = Join-Path $repoRoot "target\velopack\$Runtime\$Version"
    if (Test-Path -LiteralPath $stageDir) {
        Remove-Item -LiteralPath $stageDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

    foreach ($fileName in @("kumorust.exe", "kumorust-bootstrap.exe")) {
        $source = Join-Path $releaseDir $fileName
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Required executable was not produced: $source"
        }
        Copy-Item -LiteralPath $source -Destination $stageDir
    }

    $bootstrapDll = Get-ChildItem -LiteralPath $releaseDir -File |
        Where-Object { $_.Name -ieq "Microsoft.WindowsAppRuntime.Bootstrap.dll" } |
        Select-Object -First 1
    if ($null -eq $bootstrapDll) {
        throw "Framework-dependent bootstrap DLL was not produced in $releaseDir"
    }
    Copy-Item -LiteralPath $bootstrapDll.FullName -Destination $stageDir

    $resolvedOutputDir = if ([IO.Path]::IsPathRooted($OutputDir)) {
        $OutputDir
    } else {
        Join-Path $repoRoot $OutputDir
    }
    New-Item -ItemType Directory -Path $resolvedOutputDir -Force | Out-Null

    & vpk pack `
        --packId KumoRust `
        --packVersion $Version `
        --packDir $stageDir `
        --mainExe kumorust-bootstrap.exe `
        --packTitle KumoRust `
        --channel win `
        --runtime $Runtime `
        --noInst `
        --outputDir $resolvedOutputDir
    if ($LASTEXITCODE -ne 0) {
        throw "vpk pack failed with exit code $LASTEXITCODE."
    }

    Write-Host "Portable Velopack release written to $resolvedOutputDir"
}
finally {
    Pop-Location
}
