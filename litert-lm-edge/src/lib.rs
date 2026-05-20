mod conversation;
mod error;
mod input;
mod message;
mod stream;
#[cfg(feature = "tokio")]
mod tokio_runtime;
mod tool;

pub use conversation::{Conversation, ConversationConfig, SendOptions};
pub use error::{Error, Result};
pub use input::InputData;
pub use message::{Content, Message, Role, ToolCall, ToolCallFunction};
pub use stream::{StreamEvent, TextStream};
#[cfg(feature = "tokio")]
pub use tokio_runtime::{TokioConversation, TokioEngine, TokioSession, TokioTextStream};
pub use tool::{JsonTool, Tool, ToolDefinition};

pub(crate) use litert_lm_edge_sys as ffi;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

pub struct EngineBuilder {
    model_path: PathBuf,
    backend: Backend,
    vision_backend: Option<Backend>,
    audio_backend: Option<Backend>,
    max_num_tokens: Option<i32>,
    parallel_file_section_loading: Option<bool>,
    cache_dir: Option<PathBuf>,
    prefill_chunk_size: Option<i32>,
    max_num_images: Option<i32>,
    dispatch_lib_dir: Option<PathBuf>,
    activation_data_type: Option<i32>,
    multi_token_prediction: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Gpu,
    Custom(String),
}

impl Backend {
    fn as_str(&self) -> &str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Custom(value) => value,
        }
    }
}

pub struct Engine {
    raw: NonNull<ffi::LiteRtLmEngine>,
}

pub struct Session<'engine> {
    raw: NonNull<ffi::LiteRtLmSession>,
    _engine: PhantomData<&'engine Engine>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    pub max_output_tokens: Option<i32>,
    pub apply_prompt_template: Option<bool>,
    pub sampler: Option<SamplerParams>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplerParams {
    pub sampler_type: SamplerType,
    pub top_k: i32,
    pub top_p: f32,
    pub temperature: f32,
    pub seed: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerType {
    Unspecified,
    TopK,
    TopP,
    Greedy,
}

impl Engine {
    pub fn builder(model_path: impl AsRef<Path>) -> EngineBuilder {
        EngineBuilder {
            model_path: model_path.as_ref().to_path_buf(),
            backend: Backend::Cpu,
            vision_backend: None,
            audio_backend: None,
            max_num_tokens: None,
            parallel_file_section_loading: None,
            cache_dir: None,
            prefill_chunk_size: None,
            max_num_images: None,
            dispatch_lib_dir: None,
            activation_data_type: None,
            multi_token_prediction: None,
        }
    }

    pub fn create_session(&self, config: SessionConfig) -> Result<Session<'_>> {
        let config = RawSessionConfig::new(config)?;
        // SAFETY: self.raw is a valid engine owned by Engine. The optional config pointer is
        // either null or a live LiteRT-LM session config for the duration of this call.
        let raw =
            unsafe { ffi::litert_lm_engine_create_session(self.raw.as_ptr(), config.as_mut_ptr()) };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_engine_create_session"))?;
        Ok(Session {
            raw,
            _engine: PhantomData,
        })
    }

    pub fn create_conversation(&self, config: ConversationConfig) -> Result<Conversation<'_>> {
        conversation::Conversation::new(self, config)
    }

    pub(crate) fn raw(&self) -> NonNull<ffi::LiteRtLmEngine> {
        self.raw
    }
}

impl EngineBuilder {
    #[cfg(feature = "tokio")]
    pub async fn build_tokio(self) -> Result<tokio_runtime::TokioEngine> {
        tokio_runtime::TokioEngine::new(self).await
    }

    pub fn backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }

    pub fn vision_backend(mut self, backend: Backend) -> Self {
        self.vision_backend = Some(backend);
        self
    }

    pub fn audio_backend(mut self, backend: Backend) -> Self {
        self.audio_backend = Some(backend);
        self
    }

    pub fn max_num_tokens(mut self, max_num_tokens: i32) -> Self {
        self.max_num_tokens = Some(max_num_tokens);
        self
    }

    pub fn parallel_file_section_loading(mut self, enabled: bool) -> Self {
        self.parallel_file_section_loading = Some(enabled);
        self
    }

    pub fn cache_dir(mut self, cache_dir: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(cache_dir.as_ref().to_path_buf());
        self
    }

    pub fn prefill_chunk_size(mut self, prefill_chunk_size: i32) -> Self {
        self.prefill_chunk_size = Some(prefill_chunk_size);
        self
    }

    pub fn max_num_images(mut self, max_num_images: i32) -> Self {
        self.max_num_images = Some(max_num_images);
        self
    }

    pub fn dispatch_lib_dir(mut self, dispatch_lib_dir: impl AsRef<Path>) -> Self {
        self.dispatch_lib_dir = Some(dispatch_lib_dir.as_ref().to_path_buf());
        self
    }

    pub fn activation_data_type(mut self, activation_data_type: i32) -> Self {
        self.activation_data_type = Some(activation_data_type);
        self
    }

    pub fn multi_token_prediction(mut self, enabled: bool) -> Self {
        self.multi_token_prediction = Some(enabled);
        self
    }

    pub fn build(self) -> Result<Engine> {
        let model_path = path_to_cstring(&self.model_path)?;
        let backend = CString::new(self.backend.as_str())?;
        let vision_backend = optional_backend_cstring(self.vision_backend.as_ref())?;
        let audio_backend = optional_backend_cstring(self.audio_backend.as_ref())?;
        // SAFETY: model_path and backend strings are valid NUL-terminated strings for the
        // duration of the call. Optional vision/audio backend pointers are null or valid strings.
        let settings = unsafe {
            ffi::litert_lm_engine_settings_create(
                model_path.as_ptr(),
                backend.as_ptr(),
                optional_cstring_ptr(vision_backend.as_ref()),
                optional_cstring_ptr(audio_backend.as_ref()),
            )
        };
        let settings =
            NonNull::new(settings).ok_or(Error::NullPointer("litert_lm_engine_settings_create"))?;
        let settings = RawEngineSettings { raw: settings };

        if let Some(max_num_tokens) = self.max_num_tokens {
            // SAFETY: settings.raw is a valid settings pointer owned by RawEngineSettings.
            unsafe {
                ffi::litert_lm_engine_settings_set_max_num_tokens(
                    settings.raw.as_ptr(),
                    max_num_tokens,
                );
            }
        }
        if let Some(enabled) = self.parallel_file_section_loading {
            // SAFETY: settings.raw is a valid settings pointer owned by RawEngineSettings.
            unsafe {
                ffi::litert_lm_engine_settings_set_parallel_file_section_loading(
                    settings.raw.as_ptr(),
                    enabled,
                );
            }
        }
        let cache_dir = self
            .cache_dir
            .as_ref()
            .map(|path| path_to_cstring(path))
            .transpose()?;
        if let Some(cache_dir) = cache_dir.as_ref() {
            // SAFETY: settings.raw is valid and cache_dir is a live NUL-terminated string.
            unsafe {
                ffi::litert_lm_engine_settings_set_cache_dir(
                    settings.raw.as_ptr(),
                    cache_dir.as_ptr(),
                );
            }
        }
        if let Some(prefill_chunk_size) = self.prefill_chunk_size {
            // SAFETY: settings.raw is a valid settings pointer owned by RawEngineSettings.
            unsafe {
                ffi::litert_lm_engine_settings_set_prefill_chunk_size(
                    settings.raw.as_ptr(),
                    prefill_chunk_size,
                );
            }
        }
        if let Some(max_num_images) = self.max_num_images {
            // SAFETY: settings.raw is a valid settings pointer owned by RawEngineSettings.
            unsafe {
                ffi::litert_lm_engine_settings_set_max_num_images(
                    settings.raw.as_ptr(),
                    max_num_images,
                );
            }
        }
        let dispatch_lib_dir = self
            .dispatch_lib_dir
            .as_ref()
            .map(|path| path_to_cstring(path))
            .transpose()?;
        if let Some(dispatch_lib_dir) = dispatch_lib_dir.as_ref() {
            // SAFETY: settings.raw is valid and dispatch_lib_dir is a live NUL-terminated string.
            unsafe {
                ffi::litert_lm_engine_settings_set_litert_dispatch_lib_dir(
                    settings.raw.as_ptr(),
                    dispatch_lib_dir.as_ptr(),
                );
            }
        }
        if let Some(activation_data_type) = self.activation_data_type {
            // SAFETY: settings.raw is a valid settings pointer owned by RawEngineSettings.
            unsafe {
                ffi::litert_lm_engine_settings_set_activation_data_type(
                    settings.raw.as_ptr(),
                    activation_data_type,
                );
            }
        }
        if let Some(enabled) = self.multi_token_prediction {
            // SAFETY: settings.raw is a valid settings pointer owned by RawEngineSettings.
            unsafe {
                ffi::litert_lm_engine_settings_set_enable_speculative_decoding(
                    settings.raw.as_ptr(),
                    enabled,
                );
            }
        }

        // SAFETY: settings.raw is a valid settings pointer and remains live for the call.
        let raw = unsafe { ffi::litert_lm_engine_create(settings.raw.as_ptr()) };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_engine_create"))?;
        Ok(Engine { raw })
    }
}

impl<'engine> Session<'engine> {
    pub fn generate_text(&mut self, prompt: &str) -> Result<String> {
        self.generate_content(&[InputData::Text(prompt.to_owned())])
    }

    pub fn generate_content(&mut self, inputs: &[InputData]) -> Result<String> {
        let input = input::OwnedInputs::new(inputs)?;
        // SAFETY: self.raw is a valid session. input.as_ffi() points at bytes owned by input,
        // which remain live until the blocking call returns. LiteRT-LM returns an owned
        // responses object that must be deleted with litert_lm_responses_delete.
        let raw = unsafe {
            ffi::litert_lm_session_generate_content(
                self.raw.as_ptr(),
                input.as_ffi().as_ptr(),
                input.as_ffi().len(),
            )
        };
        let raw =
            NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_session_generate_content"))?;
        let responses = Responses { raw };
        responses.text_at(0)
    }

    pub fn generate_text_stream(&mut self, prompt: &str) -> Result<TextStream<'_>> {
        self.generate_content_stream(&[InputData::Text(prompt.to_owned())])
    }

    pub fn generate_content_stream(&mut self, inputs: &[InputData]) -> Result<TextStream<'_>> {
        let input = input::OwnedInputs::new(inputs)?;
        stream::start_text_stream(self.raw, input, PhantomData)
    }

    pub fn cancel_process(&mut self) {
        // SAFETY: self.raw is a valid session pointer owned by this Session.
        unsafe { ffi::litert_lm_session_cancel_process(self.raw.as_ptr()) };
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by litert_lm_engine_create and is owned by Engine.
        unsafe { ffi::litert_lm_engine_delete(self.raw.as_ptr()) };
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by litert_lm_engine_create_session and is owned by
        // this Session. Streaming users must drain TextStream before dropping the session.
        unsafe { ffi::litert_lm_session_delete(self.raw.as_ptr()) };
    }
}

struct RawEngineSettings {
    raw: NonNull<ffi::LiteRtLmEngineSettings>,
}

impl Drop for RawEngineSettings {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by litert_lm_engine_settings_create and is owned here.
        unsafe { ffi::litert_lm_engine_settings_delete(self.raw.as_ptr()) };
    }
}

pub(crate) struct RawSessionConfig {
    raw: Option<NonNull<ffi::LiteRtLmSessionConfig>>,
}

impl RawSessionConfig {
    pub(crate) fn new(config: SessionConfig) -> Result<Self> {
        if config == SessionConfig::default() {
            return Ok(Self { raw: None });
        }

        // SAFETY: creates a new LiteRT-LM session config and returns null on failure.
        let raw = unsafe { ffi::litert_lm_session_config_create() };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_session_config_create"))?;
        let wrapper = Self { raw: Some(raw) };

        if let Some(max_output_tokens) = config.max_output_tokens {
            // SAFETY: raw is a valid config pointer owned by wrapper.
            unsafe {
                ffi::litert_lm_session_config_set_max_output_tokens(
                    raw.as_ptr(),
                    max_output_tokens,
                );
            }
        }
        if let Some(apply_prompt_template) = config.apply_prompt_template {
            // SAFETY: raw is a valid config pointer owned by wrapper.
            unsafe {
                ffi::litert_lm_session_config_set_apply_prompt_template(
                    raw.as_ptr(),
                    apply_prompt_template,
                );
            }
        }
        if let Some(sampler) = config.sampler {
            let ffi_sampler = sampler.into_ffi();
            // SAFETY: raw is valid and ffi_sampler lives for the duration of this call.
            unsafe {
                ffi::litert_lm_session_config_set_sampler_params(raw.as_ptr(), &ffi_sampler);
            }
        }

        Ok(wrapper)
    }

    fn as_mut_ptr(&self) -> *mut ffi::LiteRtLmSessionConfig {
        self.raw.map_or(std::ptr::null_mut(), |raw| raw.as_ptr())
    }

    pub(crate) fn as_ptr(&self) -> Option<NonNull<ffi::LiteRtLmSessionConfig>> {
        self.raw
    }
}

impl Drop for RawSessionConfig {
    fn drop(&mut self) {
        if let Some(raw) = self.raw {
            // SAFETY: raw was returned by litert_lm_session_config_create and is owned here.
            unsafe { ffi::litert_lm_session_config_delete(raw.as_ptr()) };
        }
    }
}

impl PartialEq for SessionConfig {
    fn eq(&self, other: &Self) -> bool {
        self.max_output_tokens == other.max_output_tokens
            && self.apply_prompt_template == other.apply_prompt_template
            && self.sampler == other.sampler
    }
}

impl SamplerParams {
    fn into_ffi(self) -> ffi::LiteRtLmSamplerParams {
        ffi::LiteRtLmSamplerParams {
            type_: match self.sampler_type {
                SamplerType::Unspecified => ffi::kLiteRtLmSamplerTypeUnspecified,
                SamplerType::TopK => ffi::kLiteRtLmSamplerTypeTopK,
                SamplerType::TopP => ffi::kLiteRtLmSamplerTypeTopP,
                SamplerType::Greedy => ffi::kLiteRtLmSamplerTypeGreedy,
            },
            top_k: self.top_k,
            top_p: self.top_p,
            temperature: self.temperature,
            seed: self.seed,
        }
    }
}

struct Responses {
    raw: NonNull<ffi::LiteRtLmResponses>,
}

impl Responses {
    fn text_at(&self, index: i32) -> Result<String> {
        // SAFETY: self.raw is a valid responses pointer owned by Responses.
        let count = unsafe { ffi::litert_lm_responses_get_num_candidates(self.raw.as_ptr()) };
        if index < 0 || index >= count {
            return Err(Error::NullPointer(
                "litert_lm_responses_get_response_text_at",
            ));
        }
        // SAFETY: self.raw is valid and the returned string is owned by responses. It is copied
        // into a Rust String before responses is dropped.
        let ptr =
            unsafe { ffi::litert_lm_responses_get_response_text_at(self.raw.as_ptr(), index) };
        if ptr.is_null() {
            return Err(Error::NullPointer(
                "litert_lm_responses_get_response_text_at",
            ));
        }
        // SAFETY: ptr is documented as a null-terminated UTF-8 response string valid while
        // responses is alive.
        let text = unsafe { CStr::from_ptr(ptr) }.to_str()?.to_owned();
        Ok(text)
    }
}

impl Drop for Responses {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by LiteRT-LM and is owned by Responses.
        unsafe { ffi::litert_lm_responses_delete(self.raw.as_ptr()) };
    }
}

fn path_to_cstring(path: &Path) -> Result<CString> {
    let value = path.to_str().ok_or(Error::ModelPath)?;
    Ok(CString::new(value)?)
}

fn optional_backend_cstring(backend: Option<&Backend>) -> Result<Option<CString>> {
    backend
        .map(|backend| CString::new(backend.as_str()))
        .transpose()
        .map_err(Into::into)
}

fn optional_cstring_ptr(value: Option<&CString>) -> *const std::ffi::c_char {
    value.map_or(std::ptr::null(), |value| value.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_interior_nul() {
        let err = match optional_backend_cstring(Some(&Backend::Custom("bad\0backend".to_owned())))
        {
            Ok(_) => panic!("interior NUL should be rejected"),
            Err(err) => err,
        };
        assert!(matches!(err, Error::Nul(_)));
    }

    #[test]
    fn default_session_config_uses_null_pointer() {
        let config = RawSessionConfig::new(SessionConfig::default()).unwrap();
        assert!(config.as_mut_ptr().is_null());
    }

    #[test]
    fn backend_strings_match_litert_lm_api() {
        assert_eq!(Backend::Cpu.as_str(), "cpu");
        assert_eq!(Backend::Gpu.as_str(), "gpu");
        assert_eq!(Backend::Custom("npu".to_owned()).as_str(), "npu");
    }
}
