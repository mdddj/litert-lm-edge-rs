use std::ffi::{c_char, c_void};

pub const kLiteRtLmInputDataTypeText: LiteRtLmInputDataType = 0;
pub const kLiteRtLmInputDataTypeImage: LiteRtLmInputDataType = 1;
pub const kLiteRtLmInputDataTypeImageEnd: LiteRtLmInputDataType = 2;
pub const kLiteRtLmInputDataTypeAudio: LiteRtLmInputDataType = 3;
pub const kLiteRtLmInputDataTypeAudioEnd: LiteRtLmInputDataType = 4;
pub const kLiteRtLmSamplerTypeUnspecified: LiteRtLmSamplerType = 0;
pub const kLiteRtLmSamplerTypeTopK: LiteRtLmSamplerType = 1;
pub const kLiteRtLmSamplerTypeTopP: LiteRtLmSamplerType = 2;
pub const kLiteRtLmSamplerTypeGreedy: LiteRtLmSamplerType = 3;

pub enum LiteRtLmEngine {}
pub enum LiteRtLmEngineSettings {}
pub enum LiteRtLmSession {}
pub enum LiteRtLmSessionConfig {}
pub enum LiteRtLmResponses {}
pub enum LiteRtLmConversation {}
pub enum LiteRtLmConversationConfig {}
pub enum LiteRtLmConversationOptionalArgs {}
pub enum LiteRtLmJsonResponse {}

pub type LiteRtLmInputDataType = u32;
pub type LiteRtLmSamplerType = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LiteRtLmInputData {
    pub type_: LiteRtLmInputDataType,
    pub data: *const c_void,
    pub size: usize,
}

impl Default for LiteRtLmInputData {
    fn default() -> Self {
        Self {
            type_: kLiteRtLmInputDataTypeText,
            data: std::ptr::null(),
            size: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LiteRtLmSamplerParams {
    pub type_: LiteRtLmSamplerType,
    pub top_k: i32,
    pub top_p: f32,
    pub temperature: f32,
    pub seed: i32,
}

impl Default for LiteRtLmSamplerParams {
    fn default() -> Self {
        Self {
            type_: kLiteRtLmSamplerTypeUnspecified,
            top_k: 0,
            top_p: 0.0,
            temperature: 0.0,
            seed: 0,
        }
    }
}

pub type LiteRtLmStreamCallback = Option<
    unsafe extern "C" fn(
        callback_data: *mut c_void,
        chunk: *const c_char,
        is_final: bool,
        error_msg: *const c_char,
    ),
>;

extern "C" {
    pub fn litert_lm_set_min_log_level(level: i32);

    pub fn litert_lm_engine_settings_create(
        model_path: *const c_char,
        backend_str: *const c_char,
        vision_backend_str: *const c_char,
        audio_backend_str: *const c_char,
    ) -> *mut LiteRtLmEngineSettings;
    pub fn litert_lm_engine_settings_delete(settings: *mut LiteRtLmEngineSettings);
    pub fn litert_lm_engine_settings_set_max_num_tokens(
        settings: *mut LiteRtLmEngineSettings,
        max_num_tokens: i32,
    );
    pub fn litert_lm_engine_settings_set_parallel_file_section_loading(
        settings: *mut LiteRtLmEngineSettings,
        parallel_file_section_loading: bool,
    );
    pub fn litert_lm_engine_settings_set_cache_dir(
        settings: *mut LiteRtLmEngineSettings,
        cache_dir: *const c_char,
    );
    pub fn litert_lm_engine_settings_set_prefill_chunk_size(
        settings: *mut LiteRtLmEngineSettings,
        prefill_chunk_size: i32,
    );
    pub fn litert_lm_engine_settings_set_max_num_images(
        settings: *mut LiteRtLmEngineSettings,
        max_num_images: i32,
    );
    pub fn litert_lm_engine_settings_set_litert_dispatch_lib_dir(
        settings: *mut LiteRtLmEngineSettings,
        lib_dir: *const c_char,
    );
    pub fn litert_lm_engine_settings_set_activation_data_type(
        settings: *mut LiteRtLmEngineSettings,
        activation_data_type_int: i32,
    );
    pub fn litert_lm_engine_settings_set_enable_speculative_decoding(
        settings: *mut LiteRtLmEngineSettings,
        enable_speculative_decoding: bool,
    );

    pub fn litert_lm_engine_create(settings: *const LiteRtLmEngineSettings) -> *mut LiteRtLmEngine;
    pub fn litert_lm_engine_delete(engine: *mut LiteRtLmEngine);
    pub fn litert_lm_engine_create_session(
        engine: *mut LiteRtLmEngine,
        config: *mut LiteRtLmSessionConfig,
    ) -> *mut LiteRtLmSession;

    pub fn litert_lm_session_config_create() -> *mut LiteRtLmSessionConfig;
    pub fn litert_lm_session_config_delete(config: *mut LiteRtLmSessionConfig);
    pub fn litert_lm_session_config_set_max_output_tokens(
        config: *mut LiteRtLmSessionConfig,
        max_output_tokens: i32,
    );
    pub fn litert_lm_session_config_set_apply_prompt_template(
        config: *mut LiteRtLmSessionConfig,
        apply_prompt_template: bool,
    );
    pub fn litert_lm_session_config_set_sampler_params(
        config: *mut LiteRtLmSessionConfig,
        sampler_params: *const LiteRtLmSamplerParams,
    );

    pub fn litert_lm_session_delete(session: *mut LiteRtLmSession);
    pub fn litert_lm_session_cancel_process(session: *mut LiteRtLmSession);
    pub fn litert_lm_session_generate_content(
        session: *mut LiteRtLmSession,
        inputs: *const LiteRtLmInputData,
        num_inputs: usize,
    ) -> *mut LiteRtLmResponses;
    pub fn litert_lm_session_generate_content_stream(
        session: *mut LiteRtLmSession,
        inputs: *const LiteRtLmInputData,
        num_inputs: usize,
        callback: LiteRtLmStreamCallback,
        callback_data: *mut c_void,
    ) -> i32;

    pub fn litert_lm_responses_delete(responses: *mut LiteRtLmResponses);
    pub fn litert_lm_responses_get_num_candidates(responses: *const LiteRtLmResponses) -> i32;
    pub fn litert_lm_responses_get_response_text_at(
        responses: *const LiteRtLmResponses,
        index: i32,
    ) -> *const c_char;

    pub fn litert_lm_conversation_config_create() -> *mut LiteRtLmConversationConfig;
    pub fn litert_lm_conversation_config_delete(config: *mut LiteRtLmConversationConfig);
    pub fn litert_lm_conversation_config_set_session_config(
        config: *mut LiteRtLmConversationConfig,
        session_config: *const LiteRtLmSessionConfig,
    );
    pub fn litert_lm_conversation_config_set_system_message(
        config: *mut LiteRtLmConversationConfig,
        system_message_json: *const c_char,
    );
    pub fn litert_lm_conversation_config_set_tools(
        config: *mut LiteRtLmConversationConfig,
        tools_json: *const c_char,
    );
    pub fn litert_lm_conversation_config_set_messages(
        config: *mut LiteRtLmConversationConfig,
        messages_json: *const c_char,
    );
    pub fn litert_lm_conversation_config_set_extra_context(
        config: *mut LiteRtLmConversationConfig,
        extra_context_json: *const c_char,
    );
    pub fn litert_lm_conversation_config_set_enable_constrained_decoding(
        config: *mut LiteRtLmConversationConfig,
        enable_constrained_decoding: bool,
    );
    pub fn litert_lm_conversation_config_set_filter_channel_content_from_kv_cache(
        config: *mut LiteRtLmConversationConfig,
        filter_channel_content_from_kv_cache: bool,
    );
    pub fn litert_lm_conversation_optional_args_create() -> *mut LiteRtLmConversationOptionalArgs;
    pub fn litert_lm_conversation_optional_args_delete(
        optional_args: *mut LiteRtLmConversationOptionalArgs,
    );
    pub fn litert_lm_conversation_optional_args_set_visual_token_budget(
        optional_args: *mut LiteRtLmConversationOptionalArgs,
        visual_token_budget: i32,
    );
    pub fn litert_lm_conversation_create(
        engine: *mut LiteRtLmEngine,
        config: *mut LiteRtLmConversationConfig,
    ) -> *mut LiteRtLmConversation;
    pub fn litert_lm_conversation_delete(conversation: *mut LiteRtLmConversation);
    pub fn litert_lm_conversation_clone(
        conversation: *mut LiteRtLmConversation,
    ) -> *mut LiteRtLmConversation;
    pub fn litert_lm_conversation_send_message(
        conversation: *mut LiteRtLmConversation,
        message_json: *const c_char,
        extra_context: *const c_char,
        optional_args: *const LiteRtLmConversationOptionalArgs,
    ) -> *mut LiteRtLmJsonResponse;
    pub fn litert_lm_json_response_delete(response: *mut LiteRtLmJsonResponse);
    pub fn litert_lm_json_response_get_string(
        response: *const LiteRtLmJsonResponse,
    ) -> *const c_char;
    pub fn litert_lm_conversation_send_message_stream(
        conversation: *mut LiteRtLmConversation,
        message_json: *const c_char,
        extra_context: *const c_char,
        optional_args: *const LiteRtLmConversationOptionalArgs,
        callback: LiteRtLmStreamCallback,
        callback_data: *mut c_void,
    ) -> i32;
    pub fn litert_lm_conversation_render_message_to_string(
        conversation: *mut LiteRtLmConversation,
        message_json: *const c_char,
    ) -> *const c_char;
    pub fn litert_lm_conversation_cancel_process(conversation: *mut LiteRtLmConversation);
}
