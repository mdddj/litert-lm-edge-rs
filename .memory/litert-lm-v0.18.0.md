---
name: litert-lm-v0.18.0
description: Bundled LiteRT-LM runtime tracks upstream v0.18.0 in SDK 0.2.5
type: decision
---

**Why**
Upstream `v0.18.0` (`b2f686e2ed4718fb84ec398a61dd59ca0f0aff27`) adds two C functions on the headers this crate already binds: `litert_lm_engine_settings_set_single_threaded_execution` and `litert_lm_session_config_set_enable_speculative_decoding`. Existing engine and session signatures from v0.17.1 stayed the same. `c/capabilities.h` was removed; `c/model_info.h` replaced it and is not part of the `engine.h` / `conversation.h` binding surface.

**How to apply**
Keep the default tag at `v0.18.0` in the three prepare scripts and the Linux/Windows runtime workflows. Rebuild every `vendor/<target>` from that tag before calling the upgrade done. Do not mix these libraries with a v0.17.1 runtime. Expose new engine settings through `EngineBuilder` and `SessionConfig`; leave `model_info.h` and `embedding_engine.h` unbound unless that surface is requested.

**Related**
[[README]]
