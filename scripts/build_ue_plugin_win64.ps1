param(
    [string]$PluginRoot = $env:LITERT_LM_EDGE_UE_PLUGIN_ROOT,
    [string]$StageRoot = $env:LITERT_LM_EDGE_UE_STAGE_ROOT
)

$ErrorActionPreference = "Stop"

$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
$Target = "x86_64-pc-windows-msvc"
if ($PluginRoot) {
    $ThirdPartyRoot = Join-Path $PluginRoot "ThirdParty/LiteRtLmEdge"
} elseif ($StageRoot) {
    $ThirdPartyRoot = $StageRoot
} else {
    throw "Pass -PluginRoot, pass -StageRoot, or set LITERT_LM_EDGE_UE_PLUGIN_ROOT/LITERT_LM_EDGE_UE_STAGE_ROOT."
}

$WinBinDir = Join-Path $ThirdPartyRoot "bin/Win64"
$WinLibDir = Join-Path $ThirdPartyRoot "lib/Win64"
$IncludeDir = Join-Path $ThirdPartyRoot "include"
$VendorDir = Join-Path $WorkspaceRoot "litert-lm-edge-sys/vendor/windows-x86_64"
$ReleaseDir = Join-Path $WorkspaceRoot "target/$Target/release"

if ($PluginRoot -and !(Test-Path $PluginRoot)) {
    throw "UE plugin root does not exist: $PluginRoot"
}

rustup target add $Target

Push-Location $WorkspaceRoot
try {
    cargo build --release -p litert-lm-edge-ue-ffi --target $Target
}
finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path $WinBinDir, $WinLibDir, $IncludeDir | Out-Null

$BridgeDll = Join-Path $ReleaseDir "litert_lm_edge_ue_ffi.dll"
$BridgeLibCandidates = @(
    (Join-Path $ReleaseDir "litert_lm_edge_ue_ffi.dll.lib"),
    (Join-Path $ReleaseDir "litert_lm_edge_ue_ffi.lib")
)
$BridgeLib = $BridgeLibCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if (!(Test-Path $BridgeDll)) {
    throw "Rust bridge DLL was not produced: $BridgeDll"
}
if (!$BridgeLib) {
    throw "Rust bridge import library was not produced. Checked: $($BridgeLibCandidates -join ', ')"
}

Copy-Item -Force (Join-Path $WorkspaceRoot "litert-lm-edge-ue-ffi/include/litert_lm_edge_ue_ffi.h") $IncludeDir
Copy-Item -Force $BridgeDll (Join-Path $WinBinDir "litert_lm_edge_ue_ffi.dll")
Copy-Item -Force $BridgeLib (Join-Path $WinLibDir (Split-Path -Leaf $BridgeLib))

Copy-Item -Force (Join-Path $VendorDir "*.dll") $WinBinDir
Copy-Item -Force (Join-Path $VendorDir "litert_lm_c_api.lib") $WinLibDir

Write-Host "Copied Win64 LiteRT-LM Edge artifacts to $ThirdPartyRoot"
