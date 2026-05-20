#!/usr/bin/env bash
set -euo pipefail

TAG="${LITERT_LM_TAG:-v0.12.0}"
REPO_URL="${LITERT_LM_REPO_URL:-https://github.com/google-ai-edge/LiteRT-LM.git}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${LITERT_LM_BUILD_CACHE:-${ROOT_DIR}/.litert-lm-build}"
SRC_DIR="${CACHE_DIR}/LiteRT-LM"
VENDOR_DIR="${ROOT_DIR}/litert-lm-edge-sys/vendor/darwin-arm64"
VENDOR_BUILD_DIR="${SRC_DIR}/litert_lm_c_api_vendor"
BUILD_FILE="${VENDOR_BUILD_DIR}/BUILD.bazel"
LIB_NAME="liblitert_lm_c_api.dylib"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "This script builds the bundled runtime for Apple Silicon macOS only." >&2
  exit 1
fi

if command -v bazelisk >/dev/null 2>&1; then
  BAZEL=(bazelisk)
elif command -v bazel >/dev/null 2>&1; then
  BAZEL=(bazel)
else
  echo "bazelisk or bazel is required to build LiteRT-LM." >&2
  exit 1
fi

mkdir -p "${CACHE_DIR}" "${VENDOR_DIR}"

if [[ -d "${SRC_DIR}/.git" ]]; then
  git -C "${SRC_DIR}" fetch --tags --depth 1 origin "${TAG}"
else
  git clone --depth 1 --branch "${TAG}" "${REPO_URL}" "${SRC_DIR}"
fi

git -C "${SRC_DIR}" checkout --detach "${TAG}"
COMMIT="$(git -C "${SRC_DIR}" rev-parse HEAD)"

mkdir -p "${VENDOR_BUILD_DIR}"
cat >"${BUILD_FILE}" <<'EOF'
load("@rules_cc//cc:defs.bzl", "cc_binary")

cc_binary(
    name = "litert_lm_c_api_vendor",
    linkshared = True,
    linkstatic = True,
    deps = [
        "//c:engine_cpu",
    ],
)
EOF

(
  cd "${SRC_DIR}"
  "${BAZEL[@]}" build //litert_lm_c_api_vendor:litert_lm_c_api_vendor
)

rm -f "${VENDOR_DIR}"/*.dylib
install -m 755 "${SRC_DIR}/bazel-bin/litert_lm_c_api_vendor/liblitert_lm_c_api_vendor.dylib" \
  "${VENDOR_DIR}/${LIB_NAME}"
install_name_tool -id "@rpath/${LIB_NAME}" "${VENDOR_DIR}/${LIB_NAME}"

install -m 755 "${SRC_DIR}/prebuilt/macos_arm64/libGemmaModelConstraintProvider.dylib" \
  "${VENDOR_DIR}/libGemmaModelConstraintProvider.dylib"
install_name_tool -id "@rpath/libGemmaModelConstraintProvider.dylib" \
  "${VENDOR_DIR}/libGemmaModelConstraintProvider.dylib"

cat >"${VENDOR_DIR}/VERSION" <<EOF
LiteRT-LM tag: ${TAG}
LiteRT-LM commit: ${COMMIT}
Target: aarch64-apple-darwin
Bazel target: //litert_lm_c_api_vendor:litert_lm_c_api_vendor
Bazel command: ${BAZEL[*]} build //litert_lm_c_api_vendor:litert_lm_c_api_vendor
Library: ${LIB_NAME}
Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
EOF

(
  cd "${VENDOR_DIR}"
  shasum -a 256 *.dylib VERSION > SHA256SUMS
)

echo "Bundled LiteRT-LM runtime written to ${VENDOR_DIR}/${LIB_NAME}"
