#!/usr/bin/env bash
set -euo pipefail

TAG="${LITERT_LM_TAG:-v0.17.1}"
REPO_URL="${LITERT_LM_REPO_URL:-https://github.com/google-ai-edge/LiteRT-LM.git}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${LITERT_LM_BUILD_CACHE:-${ROOT_DIR}/.litert-lm-build}"
SRC_DIR="${CACHE_DIR}/LiteRT-LM-${TAG}"
VENDOR_DIR="${ROOT_DIR}/litert-lm-edge-sys/vendor/darwin-arm64"
VENDOR_BUILD_DIR="${SRC_DIR}/litert_lm_c_api_vendor"
BUILD_FILE="${VENDOR_BUILD_DIR}/BUILD.bazel"
EXPORTS_FILE="${VENDOR_BUILD_DIR}/litert_lm_c_api.exports"
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

git lfs version >/dev/null
export GIT_LFS_SKIP_SMUDGE=1


mkdir -p "${CACHE_DIR}" "${VENDOR_DIR}"

if [[ -d "${SRC_DIR}/.git" ]]; then
  git -C "${SRC_DIR}" fetch --tags --depth 1 origin "${TAG}"
else
  git clone --depth 1 --branch "${TAG}" "${REPO_URL}" "${SRC_DIR}"
fi

git -C "${SRC_DIR}" checkout --detach "${TAG}"
COMMIT="$(git -C "${SRC_DIR}" rev-parse HEAD)"
git -C "${SRC_DIR}" lfs pull --include="prebuilt/macos_arm64/*"


mkdir -p "${VENDOR_BUILD_DIR}"
python3 - "${SRC_DIR}/c" "${EXPORTS_FILE}" <<'PY'
import pathlib
import re
import sys

headers = pathlib.Path(sys.argv[1])
symbols = set()
for name in ("engine.h", "conversation.h"):
    text = (headers / name).read_text()
    symbols.update(re.findall(r"\b(litert_lm_\w+)\s*\(", text))
pathlib.Path(sys.argv[2]).write_text("".join(f"_{symbol}\n" for symbol in sorted(symbols)))
PY

cat >"${BUILD_FILE}" <<'EOF'
load("@rules_cc//cc:defs.bzl", "cc_binary")

cc_binary(
    name = "litert_lm_c_api_vendor",
    linkshared = True,
    linkstatic = True,
    linkopts = [
        "-Wl,-exported_symbols_list,$(location :litert_lm_c_api.exports)",
    ],
    data = [
        ":litert_lm_c_api.exports",
    ],
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
# An @loader_path install name lets any binary that links this library resolve
# it next to itself, with no LC_RPATH in the consumer. That matters because
# cargo:rustc-link-arg never reaches a downstream binary, so the crate cannot
# inject an rpath on the consumer's behalf. Bazel's ad-hoc signature is
# invalidated by install_name_tool, so re-sign it.
install_name_tool -id "@loader_path/${LIB_NAME}" "${VENDOR_DIR}/${LIB_NAME}"
codesign --force --sign - "${VENDOR_DIR}/${LIB_NAME}"

install -m 755 "${SRC_DIR}/prebuilt/macos_arm64/libGemmaModelConstraintProvider.dylib" \
  "${VENDOR_DIR}/libGemmaModelConstraintProvider.dylib"
# This dylib keeps its @rpath install name: only ${LIB_NAME} loads it, and that
# library already carries an @loader_path rpath. Leaving it untouched preserves
# Google's upstream code signature.
install_name_tool -id "@rpath/libGemmaModelConstraintProvider.dylib" \
  "${VENDOR_DIR}/libGemmaModelConstraintProvider.dylib"
cat >"${VENDOR_DIR}/VERSION" <<EOF
LiteRT-LM tag: ${TAG}
LiteRT-LM commit: ${COMMIT}
Target: aarch64-apple-darwin
Bazel target: //litert_lm_c_api_vendor:litert_lm_c_api_vendor
Bazel command: ${BAZEL[*]} build //litert_lm_c_api_vendor:litert_lm_c_api_vendor
Library: ${LIB_NAME}
Install name: @loader_path/${LIB_NAME}
Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
EOF

(
  cd "${VENDOR_DIR}"
  shasum -a 256 *.dylib VERSION > SHA256SUMS
)

echo "Bundled LiteRT-LM runtime written to ${VENDOR_DIR}/${LIB_NAME}"
