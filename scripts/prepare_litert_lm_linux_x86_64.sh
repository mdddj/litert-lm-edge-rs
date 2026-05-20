#!/usr/bin/env bash
set -euo pipefail

TAG="${LITERT_LM_TAG:-v0.12.0}"
REPO_URL="${LITERT_LM_REPO_URL:-https://github.com/google-ai-edge/LiteRT-LM.git}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${LITERT_LM_BUILD_CACHE:-${ROOT_DIR}/.litert-lm-build}"
BAZEL_OUTPUT_USER_ROOT="${BAZEL_OUTPUT_USER_ROOT:-/tmp/bzl}"
BAZEL_DISK_CACHE="${BAZEL_DISK_CACHE:-/tmp/bazel-disk-cache}"
BAZEL_REPOSITORY_CACHE="${BAZEL_REPOSITORY_CACHE:-/tmp/bazel-repository-cache}"
SRC_DIR="${CACHE_DIR}/LiteRT-LM"
VENDOR_DIR="${ROOT_DIR}/litert-lm-edge-sys/vendor/linux-x86_64"
VENDOR_BUILD_DIR="${SRC_DIR}/litert_lm_c_api_vendor"
BUILD_FILE="${VENDOR_BUILD_DIR}/BUILD.bazel"
LIB_NAME="liblitert_lm_c_api.so"

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  echo "This script builds the bundled runtime for Linux x86_64 only." >&2
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

if ! command -v patchelf >/dev/null 2>&1; then
  echo "patchelf is required to set the Linux runtime rpath." >&2
  exit 1
fi

enable_engine_cpu_alwayslink() {
  local build_file="$1"
  python3 - "$build_file" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
pattern = r'cc_library\(\s+name = "engine_cpu",.*?\n\)'
match = re.search(pattern, text, flags=re.S)
if not match:
    raise SystemExit(f"Could not find the //c:engine_cpu target in {path}")

block = match.group(0)
if re.search(r"alwayslink\s*=", block):
    print("LiteRT-LM //c:engine_cpu already has alwayslink enabled.")
    raise SystemExit(0)

block = re.sub(r'(name = "engine_cpu",\n)', r'\1    alwayslink = True,\n', block, count=1)
path.write_text(text[: match.start()] + block + text[match.end() :])
print("Patched LiteRT-LM //c:engine_cpu with alwayslink = True so the C API exports are retained.")
PY
}

mkdir -p "${CACHE_DIR}" "${VENDOR_DIR}" "${BAZEL_OUTPUT_USER_ROOT}" \
  "${BAZEL_DISK_CACHE}" "${BAZEL_REPOSITORY_CACHE}"

if [[ -d "${SRC_DIR}/.git" ]]; then
  git -C "${SRC_DIR}" fetch --tags --depth 1 origin "${TAG}"
else
  git clone --depth 1 --branch "${TAG}" "${REPO_URL}" "${SRC_DIR}"
fi

git -C "${SRC_DIR}" checkout --detach "${TAG}"
COMMIT="$(git -C "${SRC_DIR}" rev-parse HEAD)"

enable_engine_cpu_alwayslink "${SRC_DIR}/c/BUILD"

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
  "${BAZEL[@]}" "--output_user_root=${BAZEL_OUTPUT_USER_ROOT}" \
    build //litert_lm_c_api_vendor:litert_lm_c_api_vendor \
    --config=linux \
    "--disk_cache=${BAZEL_DISK_CACHE}" \
    "--repository_cache=${BAZEL_REPOSITORY_CACHE}"
)

rm -f "${VENDOR_DIR}"/*.so
install -m 755 "${SRC_DIR}/bazel-bin/litert_lm_c_api_vendor/liblitert_lm_c_api_vendor.so" \
  "${VENDOR_DIR}/${LIB_NAME}"
patchelf --set-soname "${LIB_NAME}" --set-rpath '$ORIGIN' "${VENDOR_DIR}/${LIB_NAME}"

PREBUILT_DIR="${SRC_DIR}/prebuilt/linux_x86_64"
for so in \
  libGemmaModelConstraintProvider.so \
  libLiteRt.so \
  libLiteRtTopKWebGpuSampler.so \
  libLiteRtWebGpuAccelerator.so; do
  if [[ -f "${PREBUILT_DIR}/${so}" ]]; then
    install -m 755 "${PREBUILT_DIR}/${so}" "${VENDOR_DIR}/${so}"
    patchelf --set-rpath '$ORIGIN' "${VENDOR_DIR}/${so}" || true
  fi
done

if ! nm -D --defined-only "${VENDOR_DIR}/${LIB_NAME}" | grep -q ' litert_lm_'; then
  echo "No litert_lm_* exports were found in ${VENDOR_DIR}/${LIB_NAME}." >&2
  nm -D --defined-only "${VENDOR_DIR}/${LIB_NAME}" | head -120 >&2
  exit 1
fi

cat >"${VENDOR_DIR}/VERSION" <<EOF
LiteRT-LM tag: ${TAG}
LiteRT-LM commit: ${COMMIT}
Target: x86_64-unknown-linux-gnu
Bazel target: //litert_lm_c_api_vendor:litert_lm_c_api_vendor
Bazel command: ${BAZEL[*]} --output_user_root=${BAZEL_OUTPUT_USER_ROOT} build //litert_lm_c_api_vendor:litert_lm_c_api_vendor --config=linux --disk_cache=${BAZEL_DISK_CACHE} --repository_cache=${BAZEL_REPOSITORY_CACHE}
Library: ${LIB_NAME}
Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
EOF

(
  cd "${VENDOR_DIR}"
  sha256sum *.so VERSION > SHA256SUMS
)

echo "Bundled LiteRT-LM Linux runtime written to ${VENDOR_DIR}/${LIB_NAME}"
