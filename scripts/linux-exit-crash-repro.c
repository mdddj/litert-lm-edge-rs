// Minimal reproduction of the consumer smoke test without any Rust.
//
// Mirrors what litert_lm_edge::Engine::builder(...).build() does: create the
// engine settings, try to create an engine from a model that does not exist,
// free whatever came back, then return from main. If this segfaults on exit
// while the Rust version does too, the defect belongs to the native runtime.
#include <stdio.h>

struct LiteRtLmEngineSettings;
struct LiteRtLmEngine;

extern struct LiteRtLmEngineSettings *litert_lm_engine_settings_create(
    const char *model_path,
    const char *backend_str,
    const char *vision_backend_str,
    const char *audio_backend_str);
extern void litert_lm_engine_settings_delete(struct LiteRtLmEngineSettings *settings);
extern struct LiteRtLmEngine *litert_lm_engine_create(
    const struct LiteRtLmEngineSettings *settings);
extern void litert_lm_engine_delete(struct LiteRtLmEngine *engine);

int main(void) {
    struct LiteRtLmEngineSettings *settings =
        litert_lm_engine_settings_create("/nonexistent/model.litertlm", "cpu", NULL, NULL);
    if (settings == NULL) {
        fprintf(stderr, "litert_lm_engine_settings_create returned NULL\n");
        return 2;
    }

    struct LiteRtLmEngine *engine = litert_lm_engine_create(settings);
    printf("engine pointer: %p\n", (void *)engine);
    if (engine != NULL) {
        litert_lm_engine_delete(engine);
    }
    litert_lm_engine_settings_delete(settings);

    printf("returning from main\n");
    fflush(stdout);
    return 0;
}
