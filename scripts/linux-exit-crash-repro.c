// Minimal reproduction of the consumer smoke test without any Rust.
//
// Mirrors what litert_lm_edge::Engine::builder(...).build() does: create the
// engine settings, try to create an engine from a model that does not exist,
// free whatever came back, then return from main. If this segfaults on exit
// while the Rust version does too, the defect belongs to the native runtime.
//
// Usage: repro [mode]
//   0  no API calls at all; the shared object is still loaded by the loader
//   1  create the settings only, never free them
//   2  create and free the settings
//   3  create the settings, attempt an engine, free both (default)
//
// The modes narrow where the crash comes from: loading the library, creating
// the settings, or tearing either down.
#include <stdio.h>
#include <stdlib.h>

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

int main(int argc, char **argv) {
    int mode = 3;
    if (argc > 1) {
        mode = atoi(argv[1]);
    }
    printf("mode %d\n", mode);
    fflush(stdout);

    if (mode == 0) {
        printf("returning from main\n");
        fflush(stdout);
        return 0;
    }

    struct LiteRtLmEngineSettings *settings =
        litert_lm_engine_settings_create("/nonexistent/model.litertlm", "cpu", NULL, NULL);
    printf("settings pointer: %p\n", (void *)settings);
    fflush(stdout);
    if (settings == NULL) {
        fprintf(stderr, "litert_lm_engine_settings_create returned NULL\n");
        return 2;
    }
    if (mode == 1) {
        printf("returning from main without freeing the settings\n");
        fflush(stdout);
        return 0;
    }
    if (mode == 2) {
        litert_lm_engine_settings_delete(settings);
        printf("returning from main after freeing the settings\n");
        fflush(stdout);
        return 0;
    }

    struct LiteRtLmEngine *engine = litert_lm_engine_create(settings);
    printf("engine pointer: %p\n", (void *)engine);
    fflush(stdout);
    if (engine != NULL) {
        litert_lm_engine_delete(engine);
    }
    litert_lm_engine_settings_delete(settings);

    printf("returning from main\n");
    fflush(stdout);
    return 0;
}
