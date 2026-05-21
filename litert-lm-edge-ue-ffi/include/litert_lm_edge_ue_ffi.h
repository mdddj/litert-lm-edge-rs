#ifndef LITERT_LM_EDGE_UE_FFI_H
#define LITERT_LM_EDGE_UE_FFI_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct LiteRtLmEdgeStreamHandle LiteRtLmEdgeStreamHandle;

typedef void (*LiteRtLmEdgeTokenCallback)(void *user_data, const char *data, size_t len);

typedef void (*LiteRtLmEdgeErrorCallback)(void *user_data,
                                          int32_t code,
                                          const char *data,
                                          size_t len);

typedef void (*LiteRtLmEdgeDoneCallback)(void *user_data, int32_t code);

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

void litert_lm_edge_ue_stream_cancel(LiteRtLmEdgeStreamHandle *handle);

void litert_lm_edge_ue_stream_free(LiteRtLmEdgeStreamHandle *handle);

const char *litert_lm_edge_ue_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* LITERT_LM_EDGE_UE_FFI_H */
