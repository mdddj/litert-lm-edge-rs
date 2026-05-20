$ErrorActionPreference = "Stop"

$Tag = if ($env:LITERT_LM_TAG) { $env:LITERT_LM_TAG } else { "v0.12.0" }
$RepoUrl = if ($env:LITERT_LM_REPO_URL) { $env:LITERT_LM_REPO_URL } else { "https://github.com/google-ai-edge/LiteRT-LM.git" }
$RootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$CacheDir = if ($env:LITERT_LM_BUILD_CACHE) { $env:LITERT_LM_BUILD_CACHE } else { Join-Path $RootDir ".litert-lm-build" }
$BazelOutputUserRoot = if ($env:BAZEL_OUTPUT_USER_ROOT) { $env:BAZEL_OUTPUT_USER_ROOT } else { "C:\bzl" }
$SrcDir = Join-Path $CacheDir "LiteRT-LM"
$VendorDir = Join-Path $RootDir "litert-lm-edge-sys\vendor\windows-x86_64"
$VendorBuildDir = Join-Path $SrcDir "litert_lm_c_api_vendor"
$BuildFile = Join-Path $VendorBuildDir "BUILD.bazel"

if (-not $IsWindows) {
    throw "This script must run on Windows x86_64 with MSVC Build Tools."
}

$Bazel = Get-Command bazelisk -ErrorAction SilentlyContinue
if (-not $Bazel) {
    $Bazel = Get-Command bazel -ErrorAction SilentlyContinue
}
if (-not $Bazel) {
    throw "bazelisk or bazel is required to build LiteRT-LM."
}

# GitHub's Windows runners preinstall Android SDK/NDK paths. LiteRT-LM's
# WORKSPACE instantiates android_ndk_repository even for this Windows CPU build,
# and the runner path can trip Bazel's repository symlink checks. Clear Android
# discovery variables so the repository stays inert for this target.
foreach ($Name in @("ANDROID_HOME", "ANDROID_SDK_ROOT", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT")) {
    Remove-Item "Env:$Name" -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force -Path $CacheDir, $VendorDir, $BazelOutputUserRoot | Out-Null

if (Test-Path (Join-Path $SrcDir ".git")) {
    git -C $SrcDir fetch --tags --depth 1 origin $Tag
} else {
    git clone --depth 1 --branch $Tag $RepoUrl $SrcDir
}

git -C $SrcDir checkout --detach $Tag
$Commit = git -C $SrcDir rev-parse HEAD

New-Item -ItemType Directory -Force -Path $VendorBuildDir | Out-Null
@'
load("@rules_cc//cc:defs.bzl", "cc_binary")

cc_binary(
    name = "litert_lm_c_api_vendor",
    linkshared = True,
    linkstatic = True,
    deps = [
        "//c:engine_cpu",
    ],
)
'@ | Set-Content -NoNewline -Encoding UTF8 $BuildFile

Push-Location $SrcDir
try {
    & $Bazel.Source "--output_user_root=$BazelOutputUserRoot" build //litert_lm_c_api_vendor:litert_lm_c_api_vendor --config=windows
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.dll")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.lib")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.exp")

$BazelBin = Join-Path $SrcDir "bazel-bin\litert_lm_c_api_vendor"
Copy-Item -Force (Join-Path $BazelBin "litert_lm_c_api_vendor.dll") (Join-Path $VendorDir "litert_lm_c_api.dll")
Copy-Item -Force (Join-Path $BazelBin "litert_lm_c_api_vendor.lib") (Join-Path $VendorDir "litert_lm_c_api.lib")

$PrebuiltDir = Join-Path $SrcDir "prebuilt\windows_x86_64"
$RuntimeDlls = @(
    "libGemmaModelConstraintProvider.dll",
    "libLiteRt.dll",
    "libLiteRtTopKWebGpuSampler.dll",
    "libLiteRtWebGpuAccelerator.dll"
)
foreach ($Dll in $RuntimeDlls) {
    $Path = Join-Path $PrebuiltDir $Dll
    if (Test-Path $Path) {
        Copy-Item -Force $Path (Join-Path $VendorDir $Dll)
    }
}

$Version = @"
LiteRT-LM tag: $Tag
LiteRT-LM commit: $Commit
Target: x86_64-pc-windows-msvc
Bazel target: //litert_lm_c_api_vendor:litert_lm_c_api_vendor
Bazel command: bazelisk --output_user_root=$BazelOutputUserRoot build //litert_lm_c_api_vendor:litert_lm_c_api_vendor --config=windows
Library: litert_lm_c_api.dll
Generated: $((Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ"))
"@
$Version | Set-Content -Encoding UTF8 (Join-Path $VendorDir "VERSION")

Push-Location $VendorDir
try {
    Get-ChildItem -File |
        Where-Object { $_.Name -eq "VERSION" -or $_.Extension -in @(".dll", ".lib") } |
        Sort-Object Name |
        ForEach-Object {
            $Hash = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant()
            "$Hash  $($_.Name)"
        } | Set-Content -Encoding UTF8 "SHA256SUMS"
} finally {
    Pop-Location
}

Write-Host "Bundled LiteRT-LM Windows runtime written to $VendorDir"
