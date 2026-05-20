$ErrorActionPreference = "Stop"

$Tag = if ($env:LITERT_LM_TAG) { $env:LITERT_LM_TAG } else { "v0.12.0" }
$RepoUrl = if ($env:LITERT_LM_REPO_URL) { $env:LITERT_LM_REPO_URL } else { "https://github.com/google-ai-edge/LiteRT-LM.git" }
$RootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$CacheDir = if ($env:LITERT_LM_BUILD_CACHE) { $env:LITERT_LM_BUILD_CACHE } else { Join-Path $RootDir ".litert-lm-build" }
$BazelOutputUserRoot = if ($env:BAZEL_OUTPUT_USER_ROOT) { $env:BAZEL_OUTPUT_USER_ROOT } else { "C:\bzl" }
$BazelDiskCache = if ($env:BAZEL_DISK_CACHE) { $env:BAZEL_DISK_CACHE } else { "C:\bazel-disk-cache" }
$BazelRepositoryCache = if ($env:BAZEL_REPOSITORY_CACHE) { $env:BAZEL_REPOSITORY_CACHE } else { "C:\bazel-repository-cache" }
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

function Get-MsvcTool {
    param([string] $ToolName)

    $Existing = Get-Command $ToolName -ErrorAction SilentlyContinue
    if ($Existing) {
        return $Existing.Source
    }

    $VsWhereCandidates = @()
    if (${env:ProgramFiles(x86)}) {
        $VsWhereCandidates += Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    }
    if ($env:ProgramFiles) {
        $VsWhereCandidates += Join-Path $env:ProgramFiles "Microsoft Visual Studio\Installer\vswhere.exe"
    }

    $VsWhere = $VsWhereCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $VsWhere) {
        throw "vswhere.exe is required to locate $ToolName.exe."
    }

    $InstallPath = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
    if ($LASTEXITCODE -ne 0 -or -not $InstallPath) {
        throw "Visual Studio C++ tools are required to locate $ToolName.exe."
    }

    $ToolsRoot = Join-Path $InstallPath "VC\Tools\MSVC"
    $Tool = Get-ChildItem -Path $ToolsRoot -Filter "$ToolName.exe" -Recurse -File |
        Where-Object { $_.FullName -match "\\bin\\Hostx64\\x64\\" } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if (-not $Tool) {
        throw "Could not find $ToolName.exe under $ToolsRoot."
    }

    return $Tool.FullName
}

function New-ImportLibraryFromDll {
    param(
        [string] $DllPath,
        [string] $OutLib,
        [string] $WorkDir
    )

    $Dumpbin = Get-MsvcTool "dumpbin"
    $LibTool = Get-MsvcTool "lib"
    $DumpbinOutput = & $Dumpbin /nologo /exports $DllPath
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed while reading exports from $DllPath."
    }

    $Exports = @(
        $DumpbinOutput |
            ForEach-Object {
                if ($_ -match "^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)") {
                    $Matches[1]
                }
            } |
            Where-Object { $_ -like "litert_lm_*" } |
            Sort-Object -Unique
    )

    if (-not $Exports) {
        throw "No litert_lm_* exports were found in $DllPath."
    }

    $DefPath = Join-Path $WorkDir "litert_lm_c_api.def"
    @("LIBRARY litert_lm_c_api.dll", "EXPORTS") + ($Exports | ForEach-Object { "    $_" }) |
        Set-Content -Encoding ASCII $DefPath

    & $LibTool /nologo "/def:$DefPath" /machine:x64 "/name:litert_lm_c_api.dll" "/out:$OutLib"
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $OutLib)) {
        throw "lib.exe failed to generate $OutLib from $DefPath."
    }

    Remove-Item -Force -ErrorAction SilentlyContinue $DefPath
    Write-Host "Generated import library $OutLib from $($Exports.Count) exported C API functions."
}

# GitHub's Windows runners preinstall Android SDK/NDK paths. LiteRT-LM's
# WORKSPACE instantiates android_ndk_repository even for this Windows CPU build,
# and the runner path can trip Bazel's repository symlink checks. Clear Android
# discovery variables so the repository stays inert for this target.
foreach ($Name in @("ANDROID_HOME", "ANDROID_SDK_ROOT", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT")) {
    Remove-Item "Env:$Name" -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force -Path $CacheDir, $VendorDir, $BazelOutputUserRoot, $BazelDiskCache, $BazelRepositoryCache | Out-Null

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
    & $Bazel.Source "--output_user_root=$BazelOutputUserRoot" build //litert_lm_c_api_vendor:litert_lm_c_api_vendor --config=windows "--disk_cache=$BazelDiskCache" "--repository_cache=$BazelRepositoryCache"
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.dll")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.lib")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.exp")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $VendorDir "*.def")

$BazelBin = Join-Path $SrcDir "bazel-bin\litert_lm_c_api_vendor"
$BuiltDll = Join-Path $BazelBin "litert_lm_c_api_vendor.dll"
if (-not (Test-Path $BuiltDll)) {
    Write-Host "Bazel output files under ${BazelBin}:"
    Get-ChildItem -Path $BazelBin -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "  $($_.FullName)"
    }
    throw "LiteRT-LM C API DLL was not found at $BuiltDll."
}

$VendorDll = Join-Path $VendorDir "litert_lm_c_api.dll"
$VendorImportLib = Join-Path $VendorDir "litert_lm_c_api.lib"
Copy-Item -Force $BuiltDll $VendorDll
New-ImportLibraryFromDll -DllPath $VendorDll -OutLib $VendorImportLib -WorkDir $VendorDir

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
Bazel command: bazelisk --output_user_root=$BazelOutputUserRoot build //litert_lm_c_api_vendor:litert_lm_c_api_vendor --config=windows --disk_cache=$BazelDiskCache --repository_cache=$BazelRepositoryCache
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
