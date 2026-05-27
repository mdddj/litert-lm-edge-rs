param(
    [ValidateSet("e2b", "e4b")]
    [string] $Model = "e2b",

    [string] $OutputDir = "models",

    [string] $BaseUrl = "https://hf-mirror.com",

    [switch] $NoResume,

    [switch] $DryRun
)

$ErrorActionPreference = "Stop"

$KnownModels = @{
    e2b = @{
        Name = "Gemma 4 E2B"
        Repo = "litert-community/gemma-4-E2B-it-litert-lm"
        File = "gemma-4-E2B-it.litertlm"
        Revision = "73d35ec36cf24347ab4eec1a46f0aafbb9c3a89d"
        Sha256 = "181938105e0eefd105961417e8da75903eacda102c4fce9ce90f50b97139a63c"
        SizeBytes = 2588147712
        Dir = "gemma-4-E2B-it-litert-lm"
    }
    e4b = @{
        Name = "Gemma 4 E4B"
        Repo = "litert-community/gemma-4-E4B-it-litert-lm"
        File = "gemma-4-E4B-it.litertlm"
        Revision = "4f479a5ff97de64f5c1711ec439a2cb89e6a8fb4"
        Sha256 = "0b2a8980ce155fd97673d8e820b4d29d9c7d99b8fa6806f425d969b145bd52e0"
        SizeBytes = 3659530240
        Dir = "gemma-4-E4B-it-litert-lm"
    }
}

function Format-ByteCount {
    param([Int64] $Bytes)

    $Units = @("B", "KiB", "MiB", "GiB")
    $Value = [double] $Bytes
    $Index = 0
    while ($Value -ge 1024 -and $Index -lt ($Units.Length - 1)) {
        $Value = $Value / 1024
        $Index++
    }

    return ("{0:N1} {1}" -f $Value, $Units[$Index])
}

function Assert-Sha256 {
    param(
        [string] $Path,
        [string] $Expected
    )

    Write-Host "Verifying SHA256..."
    $Actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    $ExpectedLower = $Expected.ToLowerInvariant()
    if ($Actual -ne $ExpectedLower) {
        throw "SHA256 mismatch for $Path. Expected $ExpectedLower, got $Actual."
    }
}

$Info = $KnownModels[$Model.ToLowerInvariant()]
$OutputRoot = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDir)
$ModelDir = Join-Path $OutputRoot $Info.Dir
$FinalPath = Join-Path $ModelDir $Info.File
$PartialPath = "$FinalPath.partial"
$DownloadUrl = "{0}/{1}/resolve/{2}/{3}" -f $BaseUrl.TrimEnd([char[]]"/"), $Info.Repo, $Info.Revision, $Info.File

Write-Host ("Model: {0}" -f $Info.Name)
Write-Host ("Size:  {0}" -f (Format-ByteCount $Info.SizeBytes))
Write-Host ("URL:   {0}" -f $DownloadUrl)
Write-Host ("Path:  {0}" -f $FinalPath)

if ($DryRun) {
    Write-Output $FinalPath
    exit 0
}

New-Item -ItemType Directory -Force -Path $ModelDir | Out-Null

if (Test-Path -LiteralPath $FinalPath) {
    Assert-Sha256 -Path $FinalPath -Expected $Info.Sha256
    Write-Host "Already downloaded and verified."
    Write-Output $FinalPath
    exit 0
}

if ($NoResume -and (Test-Path -LiteralPath $PartialPath)) {
    Remove-Item -Force -LiteralPath $PartialPath
}

if (Test-Path -LiteralPath $PartialPath) {
    $PartialSize = (Get-Item -LiteralPath $PartialPath).Length
    if ($PartialSize -gt $Info.SizeBytes) {
        Write-Warning "Partial file is larger than expected; removing it and starting over."
        Remove-Item -Force -LiteralPath $PartialPath
    } elseif ($PartialSize -eq $Info.SizeBytes) {
        Assert-Sha256 -Path $PartialPath -Expected $Info.Sha256
        Move-Item -Force -LiteralPath $PartialPath -Destination $FinalPath
        Write-Host "Downloaded and verified."
        Write-Output $FinalPath
        exit 0
    } elseif ($PartialSize -gt 0) {
        Write-Host ("Resuming from {0}..." -f (Format-ByteCount $PartialSize))
    }
}

$Curl = Get-Command curl.exe -ErrorAction SilentlyContinue
if (-not $Curl) {
    throw "curl.exe was not found. Windows 10/11 normally includes it. Install curl or run this script in a newer PowerShell/Windows environment."
}

$CurlArgs = @(
    "--location",
    "--fail",
    "--show-error",
    "--progress-bar",
    "--connect-timeout", "30",
    "--retry", "8",
    "--retry-delay", "3",
    "--continue-at", "-",
    "--output", $PartialPath,
    $DownloadUrl
)

& $Curl.Source @CurlArgs
if ($LASTEXITCODE -ne 0) {
    throw "curl.exe failed with exit code $LASTEXITCODE. The partial file is kept at $PartialPath; rerun the script to continue."
}

Assert-Sha256 -Path $PartialPath -Expected $Info.Sha256
Move-Item -Force -LiteralPath $PartialPath -Destination $FinalPath

Write-Host "Downloaded and verified."
Write-Output $FinalPath
