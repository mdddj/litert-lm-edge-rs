#ifndef LITERT_LM_EDGE_UE_FFI_H
#define LITERT_LM_EDGE_UE_FFI_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct LiteRtLmEdgeConversationHandle LiteRtLmEdgeConversationHandle;

typedef struct LiteRtLmEdgeStreamHandle LiteRtLmEdgeStreamHandle;

typedef void (*LiteRtLmEdgeTokenCallback)(void *user_data, const char *data, size_t len);

typedef void (*LiteRtLmEdgeErrorCallback)(void *user_data,
                                          int32_t code,
                                          const char *data,
                                          size_t len);

typedef void (*LiteRtLmEdgeDoneCallback)(void *user_data, int32_t code);

typedef struct {
  const char *const *data;
  size_t len;
} LiteRtLmEdgeStringArray;

typedef struct {
  const char *model_path;
  const char *prompt;
  LiteRtLmEdgeStringArray image_paths;
  LiteRtLmEdgeStringArray audio_paths;
  const char *backend;
  const char *vision_backend;
  const char *audio_backend;
  int32_t max_num_images;
  int32_t max_output_tokens;
} LiteRtLmEdgeMultimodalRequest;

typedef void (*LiteRtLmEdgeJsonCallback)(void *user_data, const char *data, size_t len);

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

LiteRtLmEdgeStreamHandle *litert_lm_edge_ue_stream_text_start(const char *model_path,
                                                              const char *prompt,
                                                              const char *backend,
                                                              int32_t max_output_tokens,
                                                              void *user_data,
                                                              LiteRtLmEdgeTokenCallback on_token,
                                                              LiteRtLmEdgeErrorCallback on_error,
                                                              LiteRtLmEdgeDoneCallback on_done);

LiteRtLmEdgeStreamHandle *litert_lm_edge_ue_stream_multimodal_start(const LiteRtLmEdgeMultimodalRequest *request,
                                                                    void *user_data,
                                                                    LiteRtLmEdgeTokenCallback on_token,
                                                                    LiteRtLmEdgeErrorCallback on_error,
                                                                    LiteRtLmEdgeDoneCallback on_done);

void litert_lm_edge_ue_stream_cancel(LiteRtLmEdgeStreamHandle *handle);

void litert_lm_edge_ue_stream_free(LiteRtLmEdgeStreamHandle *handle);

int32_t litert_lm_edge_ue_conversation_create(const char *model_path,
                                              const char *backend,
                                              const char *vision_backend,
                                              const char *audio_backend,
                                              int32_t max_num_images,
                                              int32_t max_output_tokens,
                                              const char *system_prompt,
                                              const char *tools_json,
                                              LiteRtLmEdgeConversationHandle **out_handle,
                                              void *user_data,
                                              LiteRtLmEdgeErrorCallback on_error);

void litert_lm_edge_ue_conversation_cancel(LiteRtLmEdgeConversationHandle *handle);

void litert_lm_edge_ue_conversation_free(LiteRtLmEdgeConversationHandle *handle);

LiteRtLmEdgeStreamHandle *litert_lm_edge_ue_conversation_send_message_start(LiteRtLmEdgeConversationHandle *handle,
                                                                            const char *prompt,
                                                                            LiteRtLmEdgeStringArray image_paths,
                                                                            LiteRtLmEdgeStringArray audio_paths,
                                                                            int32_t visual_token_budget,
                                                                            void *user_data,
                                                                            LiteRtLmEdgeTokenCallback on_text,
                                                                            LiteRtLmEdgeJsonCallback on_tool_calls,
                                                                            LiteRtLmEdgeJsonCallback on_response_json,
                                                                            LiteRtLmEdgeErrorCallback on_error,
                                                                            LiteRtLmEdgeDoneCallback on_done);

LiteRtLmEdgeStreamHandle *litert_lm_edge_ue_conversation_continue_tools_start(LiteRtLmEdgeConversationHandle *handle,
                                                                              const char *tool_results_json,
                                                                              void *user_data,
                                                                              LiteRtLmEdgeTokenCallback on_text,
                                                                              LiteRtLmEdgeJsonCallback on_tool_calls,
                                                                              LiteRtLmEdgeJsonCallback on_response_json,
                                                                              LiteRtLmEdgeErrorCallback on_error,
                                                                              LiteRtLmEdgeDoneCallback on_done);

const char *litert_lm_edge_ue_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* LITERT_LM_EDGE_UE_FFI_H */
