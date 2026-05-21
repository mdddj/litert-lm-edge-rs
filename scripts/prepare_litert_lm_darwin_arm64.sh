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

mkdir -p "${CACHE_DIR}" "${VENDOR_DIR}"

if [[ -d "${SRC_DIR}/.git" ]]; then
  git -C "${SRC_DIR}" fetch --tags --depth 1 origin "${TAG}"
else
  git clone --depth 1 --branch "${TAG}" "${REPO_URL}" "${SRC_DIR}"
fi

git -C "${SRC_DIR}" checkout --detach "${TAG}"
COMMIT="$(git -C "${SRC_DIR}" rev-parse HEAD)"

mkdir -p "${VENDOR_BUILD_DIR}"
cat >"${EXPORTS_FILE}" <<'EOF'
_litert_lm_benchmark_info_delete
_litert_lm_benchmark_info_get_decode_token_count_at
_litert_lm_benchmark_info_get_decode_tokens_per_sec_at
_litert_lm_benchmark_info_get_num_decode_turns
_litert_lm_benchmark_info_get_num_prefill_turns
_litert_lm_benchmark_info_get_prefill_token_count_at
_litert_lm_benchmark_info_get_prefill_tokens_per_sec_at
_litert_lm_benchmark_info_get_time_to_first_token
_litert_lm_benchmark_info_get_total_init_time_in_second
_litert_lm_conversation_cancel_process
_litert_lm_conversation_clone
_litert_lm_conversation_config_create
_litert_lm_conversation_config_delete
_litert_lm_conversation_config_set_enable_constrained_decoding
_litert_lm_conversation_config_set_extra_context
_litert_lm_conversation_config_set_filter_channel_content_from_kv_cache
_litert_lm_conversation_config_set_messages
_litert_lm_conversation_config_set_session_config
_litert_lm_conversation_config_set_system_message
_litert_lm_conversation_config_set_tools
_litert_lm_conversation_create
_litert_lm_conversation_delete
_litert_lm_conversation_get_benchmark_info
_litert_lm_conversation_optional_args_create
_litert_lm_conversation_optional_args_delete
_litert_lm_conversation_optional_args_set_visual_token_budget
_litert_lm_conversation_render_message_to_string
_litert_lm_conversation_send_message
_litert_lm_conversation_send_message_stream
_litert_lm_detokenize_result_delete
_litert_lm_detokenize_result_get_string
_litert_lm_engine_create
_litert_lm_engine_create_session
_litert_lm_engine_delete
_litert_lm_engine_detokenize
_litert_lm_engine_get_start_token
_litert_lm_engine_get_stop_tokens
_litert_lm_engine_settings_create
_litert_lm_engine_settings_delete
_litert_lm_engine_settings_enable_benchmark
_litert_lm_engine_settings_set_activation_data_type
_litert_lm_engine_settings_set_cache_dir
_litert_lm_engine_settings_set_enable_speculative_decoding
_litert_lm_engine_settings_set_litert_dispatch_lib_dir
_litert_lm_engine_settings_set_max_num_images
_litert_lm_engine_settings_set_max_num_tokens
_litert_lm_engine_settings_set_num_decode_tokens
_litert_lm_engine_settings_set_num_prefill_tokens
_litert_lm_engine_settings_set_parallel_file_section_loading
_litert_lm_engine_settings_set_prefill_chunk_size
_litert_lm_engine_tokenize
_litert_lm_json_response_delete
_litert_lm_json_response_get_string
_litert_lm_responses_delete
_litert_lm_responses_get_num_candidates
_litert_lm_responses_get_num_token_scores_at
_litert_lm_responses_get_response_text_at
_litert_lm_responses_get_score_at
_litert_lm_responses_get_token_length_at
_litert_lm_responses_get_token_scores_at
_litert_lm_responses_has_score_at
_litert_lm_responses_has_token_length_at
_litert_lm_responses_has_token_scores_at
_litert_lm_session_cancel_process
_litert_lm_session_config_create
_litert_lm_session_config_delete
_litert_lm_session_config_set_apply_prompt_template
_litert_lm_session_config_set_max_output_tokens
_litert_lm_session_config_set_sampler_params
_litert_lm_session_delete
_litert_lm_session_generate_content
_litert_lm_session_generate_content_stream
_litert_lm_session_get_benchmark_info
_litert_lm_session_run_decode
_litert_lm_session_run_decode_async
_litert_lm_session_run_prefill
_litert_lm_session_run_text_scoring
_litert_lm_set_min_log_level
_litert_lm_token_union_delete
_litert_lm_token_union_get_ids
_litert_lm_token_union_get_string
_litert_lm_token_union_get_type
_litert_lm_token_unions_delete
_litert_lm_token_unions_get_num_tokens
_litert_lm_token_unions_get_token_at
_litert_lm_tokenize_result_delete
_litert_lm_tokenize_result_get_num_tokens
_litert_lm_tokenize_result_get_tokens
EOF

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
