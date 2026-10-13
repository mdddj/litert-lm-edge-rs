//! Vector embeddings from `.litertlm` embedding bundles such as EmbeddingGemma 2.
//!
//! Embedding models are a separate surface from the generation engine: a
//! `.litertlm` bundle built for `EmbeddingEngine` produces vectors and has no
//! text-generation path, so it cannot be loaded with [`crate::Engine`]. The two
//! APIs share the [`crate::InputData`] input format.
//!
//! ```no_run
//! use litert_lm_edge::{EmbeddingEngine, EmbeddingTask, InputData};
//!
//! fn main() -> litert_lm_edge::Result<()> {
//!     let mut engine = EmbeddingEngine::builder("/path/to/embeddinggemma-2-740m.litertlm")
//!         .num_threads(4)
//!         .build()?;
//!
//!     let query = engine.compute_embedding(&[InputData::text(
//!         EmbeddingTask::Query.prompt("How do I reset my password?"),
//!     )])?;
//!     println!("{} dimensions", query.len());
//!     Ok(())
//! }
//! ```

use crate::input::OwnedInputs;
use crate::{
    optional_backend_cstring, optional_cstring_ptr, path_to_cstring, Backend, Error, InputData,
    Result,
};
use litert_lm_edge_sys as ffi;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

/// How inputs longer than the model's maximum signature length are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOverflowStrategy {
    /// Chunk the input, embed each chunk, and return the mean embedding.
    ChunkAndAverage,
    /// Truncate the input to the longest supported length.
    Truncate,
    /// Return an error if the input is too long.
    Error,
}

impl InputOverflowStrategy {
    fn as_raw(self) -> ffi::LiteRtLmInputOverflowStrategy {
        match self {
            Self::ChunkAndAverage => ffi::kLiteRtLmInputOverflowStrategyChunkAndAverage,
            Self::Truncate => ffi::kLiteRtLmInputOverflowStrategyTruncate,
            Self::Error => ffi::kLiteRtLmInputOverflowStrategyError,
        }
    }
}

/// Per-call embedding overrides. Any field left as `None` keeps the native
/// default, and an all-`None` value is passed to the runtime as "no options".
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EmbeddingOptions {
    /// L2-normalize the returned vector so dot product equals cosine similarity.
    pub normalize: Option<bool>,
    /// Wrap the input in the model's BOS/EOS tokens.
    pub insert_special_tokens: Option<bool>,
    pub input_overflow_strategy: Option<InputOverflowStrategy>,
    /// Override the output dimension via Matryoshka slicing.
    pub output_size: Option<i32>,
    /// Vision token budget used when the input contains images.
    pub vision_tokens_per_image: Option<i32>,
}

impl EmbeddingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn normalize(mut self, normalize: bool) -> Self {
        self.normalize = Some(normalize);
        self
    }

    pub fn insert_special_tokens(mut self, insert: bool) -> Self {
        self.insert_special_tokens = Some(insert);
        self
    }

    pub fn input_overflow_strategy(mut self, strategy: InputOverflowStrategy) -> Self {
        self.input_overflow_strategy = Some(strategy);
        self
    }

    pub fn output_size(mut self, output_size: i32) -> Self {
        self.output_size = Some(output_size);
        self
    }

    pub fn vision_tokens_per_image(mut self, tokens: i32) -> Self {
        self.vision_tokens_per_image = Some(tokens);
        self
    }
}

/// The asymmetric task prefixes EmbeddingGemma 2 expects before the input text.
///
/// The model is trained with different prefixes for indexing and querying, so
/// using the same prefix on both sides measurably degrades retrieval quality.
/// Pass the result through [`InputData::text`] (or prepend it to a multimodal
/// prompt) rather than sending the raw user string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingTask {
    /// Text being added to the index.
    Document,
    /// A user query matched against indexed documents.
    Query,
    Classification,
    Clustering,
    SentenceSimilarity,
}

impl EmbeddingTask {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Document => "task: search result | text: ",
            Self::Query => "task: search query | text: ",
            Self::Classification => "task: classification | text: ",
            Self::Clustering => "task: clustering | text: ",
            Self::SentenceSimilarity => "task: sentence similarity | text: ",
        }
    }

    /// Prepends the task prefix to `text`, trimming surrounding whitespace as
    /// the model card requires.
    pub fn prompt(self, text: &str) -> String {
        format!("{}{}", self.prefix(), text.trim())
    }
}

pub struct EmbeddingEngineBuilder {
    model_path: PathBuf,
    backend: Backend,
    vision_backend: Option<Backend>,
    audio_backend: Option<Backend>,
    num_threads: Option<i32>,
    audio_num_threads: Option<i32>,
    cache_dir: Option<PathBuf>,
    dispatch_lib_dir: Option<PathBuf>,
    vision_dispatch_lib_dir: Option<PathBuf>,
    audio_dispatch_lib_dir: Option<PathBuf>,
    max_input_length: Option<i32>,
    min_input_length: Option<i32>,
    vision_tokens_per_image: Option<i32>,
    activation_data_type: Option<i32>,
}

/// A native embedding engine. Vectors are produced by
/// [`EmbeddingEngine::compute_embedding`] and
/// [`EmbeddingEngine::compute_embedding_batch`].
pub struct EmbeddingEngine {
    raw: NonNull<ffi::LiteRtLmEmbeddingEngine>,
}

impl EmbeddingEngine {
    pub fn builder(model_path: impl AsRef<Path>) -> EmbeddingEngineBuilder {
        EmbeddingEngineBuilder {
            model_path: model_path.as_ref().to_path_buf(),
            backend: Backend::Cpu,
            vision_backend: None,
            audio_backend: None,
            num_threads: None,
            audio_num_threads: None,
            cache_dir: None,
            dispatch_lib_dir: None,
            vision_dispatch_lib_dir: None,
            audio_dispatch_lib_dir: None,
            max_input_length: None,
            min_input_length: None,
            vision_tokens_per_image: None,
            activation_data_type: None,
        }
    }

    /// Embeds a single request. `inputs` may mix text, image, and audio
    /// [`InputData`], matching the generation API's multimodal format.
    pub fn compute_embedding(&mut self, inputs: &[InputData]) -> Result<Vec<f32>> {
        self.compute_embedding_with_options(inputs, EmbeddingOptions::default())
    }

    pub fn compute_embedding_with_options(
        &mut self,
        inputs: &[InputData],
        options: EmbeddingOptions,
    ) -> Result<Vec<f32>> {
        let input = OwnedInputs::new(inputs)?;
        let options = RawEmbeddingOptions::new(options)?;
        // SAFETY: self.raw is a valid engine owned by EmbeddingEngine; input owns the native
        // input objects and their pointer array until the call returns; the options pointer is
        // either null or a live options object owned by the local binding.
        let raw = unsafe {
            ffi::litert_lm_embedding_engine_compute_embedding(
                self.raw.as_ptr(),
                input.as_ffi().as_ptr(),
                input.as_ffi().len(),
                options.as_ptr(),
            )
        };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer(
            "litert_lm_embedding_engine_compute_embedding",
        ))?;
        OwnedEmbeddingResponse { raw }.values()
    }

    /// Embeds several requests in one call. Each inner slice is one request.
    pub fn compute_embedding_batch(&mut self, batches: &[Vec<InputData>]) -> Result<Vec<Vec<f32>>> {
        self.compute_embedding_batch_with_options(batches, EmbeddingOptions::default())
    }

    pub fn compute_embedding_batch_with_options(
        &mut self,
        batches: &[Vec<InputData>],
        options: EmbeddingOptions,
    ) -> Result<Vec<Vec<f32>>> {
        // Each request needs its own owned input array, and all of them must
        // outlive the native call, so they are collected before the FFI call.
        let owned: Vec<OwnedInputs> = batches
            .iter()
            .map(|batch| OwnedInputs::new(batch))
            .collect::<Result<_>>()?;
        let pointers: Vec<*const *const ffi::LiteRtLmInputData> = owned
            .iter()
            .map(|inputs| inputs.as_ffi().as_ptr())
            .collect();
        let lengths: Vec<usize> = owned.iter().map(|inputs| inputs.as_ffi().len()).collect();
        let options = RawEmbeddingOptions::new(options)?;

        // SAFETY: self.raw is a valid engine; `owned` keeps every per-request input array alive
        // and `pointers`/`lengths` describe them for the duration of the call; the options
        // pointer is null or owned locally.
        let raw = unsafe {
            ffi::litert_lm_embedding_engine_compute_embedding_batch(
                self.raw.as_ptr(),
                pointers.as_ptr(),
                lengths.as_ptr(),
                lengths.len(),
                options.as_ptr(),
            )
        };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer(
            "litert_lm_embedding_engine_compute_embedding_batch",
        ))?;
        OwnedEmbeddingResponses { raw }.to_vec()
    }
}

impl Drop for EmbeddingEngine {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by litert_lm_embedding_engine_create and is owned here.
        unsafe { ffi::litert_lm_embedding_engine_delete(self.raw.as_ptr()) };
    }
}

impl EmbeddingEngineBuilder {
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

    pub fn num_threads(mut self, num_threads: i32) -> Self {
        self.num_threads = Some(num_threads);
        self
    }

    pub fn audio_num_threads(mut self, num_threads: i32) -> Self {
        self.audio_num_threads = Some(num_threads);
        self
    }

    pub fn cache_dir(mut self, cache_dir: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(cache_dir.as_ref().to_path_buf());
        self
    }

    pub fn dispatch_lib_dir(mut self, dispatch_lib_dir: impl AsRef<Path>) -> Self {
        self.dispatch_lib_dir = Some(dispatch_lib_dir.as_ref().to_path_buf());
        self
    }

    pub fn vision_dispatch_lib_dir(mut self, dispatch_lib_dir: impl AsRef<Path>) -> Self {
        self.vision_dispatch_lib_dir = Some(dispatch_lib_dir.as_ref().to_path_buf());
        self
    }

    pub fn audio_dispatch_lib_dir(mut self, dispatch_lib_dir: impl AsRef<Path>) -> Self {
        self.audio_dispatch_lib_dir = Some(dispatch_lib_dir.as_ref().to_path_buf());
        self
    }

    pub fn max_input_length(mut self, max_input_length: i32) -> Self {
        self.max_input_length = Some(max_input_length);
        self
    }

    pub fn min_input_length(mut self, min_input_length: i32) -> Self {
        self.min_input_length = Some(min_input_length);
        self
    }

    pub fn vision_tokens_per_image(mut self, tokens: i32) -> Self {
        self.vision_tokens_per_image = Some(tokens);
        self
    }

    pub fn activation_data_type(mut self, activation_data_type: i32) -> Self {
        self.activation_data_type = Some(activation_data_type);
        self
    }

    pub fn build(self) -> Result<EmbeddingEngine> {
        let model_path = path_to_cstring(&self.model_path)?;
        let backend = std::ffi::CString::new(self.backend.as_str())?;
        let vision_backend = optional_backend_cstring(self.vision_backend.as_ref())?;
        let audio_backend = optional_backend_cstring(self.audio_backend.as_ref())?;
        // SAFETY: every string pointer is either null or a valid NUL-terminated string that
        // outlives the call.
        let settings = unsafe {
            ffi::litert_lm_embedding_engine_settings_create(
                model_path.as_ptr(),
                backend.as_ptr(),
                optional_cstring_ptr(vision_backend.as_ref()),
                optional_cstring_ptr(audio_backend.as_ref()),
            )
        };
        let settings = NonNull::new(settings).ok_or(Error::NullPointer(
            "litert_lm_embedding_engine_settings_create",
        ))?;
        let settings = RawEmbeddingEngineSettings { raw: settings };

        macro_rules! apply_int {
            ($value:expr, $setter:ident) => {
                if let Some(value) = $value {
                    // SAFETY: settings.raw is a valid settings pointer owned by the binding.
                    unsafe { ffi::$setter(settings.raw.as_ptr(), value) };
                }
            };
        }
        apply_int!(
            self.num_threads,
            litert_lm_embedding_engine_settings_set_num_threads
        );
        apply_int!(
            self.audio_num_threads,
            litert_lm_embedding_engine_settings_set_audio_num_threads
        );
        apply_int!(
            self.max_input_length,
            litert_lm_embedding_engine_settings_set_max_input_length
        );
        apply_int!(
            self.min_input_length,
            litert_lm_embedding_engine_settings_set_min_input_length
        );
        apply_int!(
            self.vision_tokens_per_image,
            litert_lm_embedding_engine_settings_set_vision_tokens_per_image
        );

        if let Some(activation_data_type) = self.activation_data_type {
            // SAFETY: settings.raw is valid; the int maps onto the engine.h enum.
            unsafe {
                ffi::litert_lm_embedding_engine_settings_set_activation_data_type(
                    settings.raw.as_ptr(),
                    activation_data_type as _,
                );
            }
        }

        macro_rules! apply_path {
            ($value:expr, $setter:ident) => {{
                let value = $value
                    .as_ref()
                    .map(|path| path_to_cstring(path))
                    .transpose()?;
                if let Some(value) = value.as_ref() {
                    // SAFETY: settings.raw is valid and value is a live NUL-terminated string.
                    unsafe { ffi::$setter(settings.raw.as_ptr(), value.as_ptr()) };
                }
            }};
        }
        apply_path!(
            self.cache_dir,
            litert_lm_embedding_engine_settings_set_cache_dir
        );
        apply_path!(
            self.dispatch_lib_dir,
            litert_lm_embedding_engine_settings_set_litert_dispatch_lib_dir
        );
        apply_path!(
            self.vision_dispatch_lib_dir,
            litert_lm_embedding_engine_settings_set_vision_litert_dispatch_lib_dir
        );
        apply_path!(
            self.audio_dispatch_lib_dir,
            litert_lm_embedding_engine_settings_set_audio_litert_dispatch_lib_dir
        );

        // SAFETY: settings.raw is a valid settings pointer and remains live for the call. The
        // engine creation copies what it needs, so settings may be dropped afterwards.
        let raw = unsafe { ffi::litert_lm_embedding_engine_create(settings.raw.as_ptr()) };
        let raw =
            NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_embedding_engine_create"))?;
        Ok(EmbeddingEngine { raw })
    }
}

struct RawEmbeddingEngineSettings {
    raw: NonNull<ffi::LiteRtLmEmbeddingEngineSettings>,
}

impl Drop for RawEmbeddingEngineSettings {
    fn drop(&mut self) {
        // SAFETY: raw was returned by litert_lm_embedding_engine_settings_create.
        unsafe { ffi::litert_lm_embedding_engine_settings_delete(self.raw.as_ptr()) };
    }
}

struct RawEmbeddingOptions {
    raw: Option<NonNull<ffi::LiteRtLmEmbeddingOptions>>,
}

impl RawEmbeddingOptions {
    /// Keeps a null pointer for the default options so the native call falls
    /// back to its own defaults instead of allocating an options object.
    fn new(options: EmbeddingOptions) -> Result<Self> {
        if options == EmbeddingOptions::default() {
            return Ok(Self { raw: None });
        }

        // SAFETY: creates a fresh options object and returns null on failure.
        let raw = unsafe { ffi::litert_lm_embedding_options_create() };
        let raw =
            NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_embedding_options_create"))?;
        let wrapper = Self { raw: Some(raw) };

        if let Some(normalize) = options.normalize {
            // SAFETY: raw is a valid options pointer owned by wrapper.
            unsafe { ffi::litert_lm_embedding_options_set_normalize(raw.as_ptr(), normalize) };
        }
        if let Some(insert) = options.insert_special_tokens {
            // SAFETY: raw is a valid options pointer owned by wrapper.
            unsafe {
                ffi::litert_lm_embedding_options_set_insert_special_tokens(raw.as_ptr(), insert);
            }
        }
        if let Some(strategy) = options.input_overflow_strategy {
            // SAFETY: raw is a valid options pointer owned by wrapper.
            unsafe {
                ffi::litert_lm_embedding_options_set_input_overflow_strategy(
                    raw.as_ptr(),
                    strategy.as_raw(),
                );
            }
        }
        if let Some(output_size) = options.output_size {
            // SAFETY: raw is a valid options pointer owned by wrapper.
            unsafe {
                ffi::litert_lm_embedding_options_set_output_size(raw.as_ptr(), output_size);
            }
        }
        if let Some(tokens) = options.vision_tokens_per_image {
            // SAFETY: raw is a valid options pointer owned by wrapper.
            unsafe {
                ffi::litert_lm_embedding_options_set_vision_tokens_per_image(raw.as_ptr(), tokens);
            }
        }

        Ok(wrapper)
    }

    fn as_ptr(&self) -> *const ffi::LiteRtLmEmbeddingOptions {
        self.raw.map_or(std::ptr::null(), |raw| raw.as_ptr())
    }
}

impl Drop for RawEmbeddingOptions {
    fn drop(&mut self) {
        if let Some(raw) = self.raw {
            // SAFETY: raw was returned by litert_lm_embedding_options_create and is owned here.
            unsafe { ffi::litert_lm_embedding_options_delete(raw.as_ptr()) };
        }
    }
}

/// Copies a vector out of a response. The response keeps ownership of the
/// underlying buffer, which is only valid while the response is alive.
fn response_values(raw: *const ffi::LiteRtLmEmbeddingResponse) -> Result<Vec<f32>> {
    // SAFETY: raw is a live response pointer; both getters only read from it.
    let size = unsafe { ffi::litert_lm_embedding_response_get_size(raw) };
    // SAFETY: raw is live and the returned buffer holds `size` floats owned by the response.
    let values = unsafe { ffi::litert_lm_embedding_response_get_values(raw) };
    if values.is_null() {
        if size == 0 {
            return Ok(Vec::new());
        }
        return Err(Error::NullPointer(
            "litert_lm_embedding_response_get_values",
        ));
    }
    // SAFETY: values points to at least `size` initialized f32 values, and the slice is copied
    // before the response can be dropped.
    Ok(unsafe { std::slice::from_raw_parts(values, size) }.to_vec())
}

struct OwnedEmbeddingResponse {
    raw: NonNull<ffi::LiteRtLmEmbeddingResponse>,
}

impl OwnedEmbeddingResponse {
    fn values(&self) -> Result<Vec<f32>> {
        response_values(self.raw.as_ptr())
    }
}

impl Drop for OwnedEmbeddingResponse {
    fn drop(&mut self) {
        // SAFETY: raw was returned by compute_embedding and is owned here.
        unsafe { ffi::litert_lm_embedding_response_delete(self.raw.as_ptr()) };
    }
}

struct OwnedEmbeddingResponses {
    raw: NonNull<ffi::LiteRtLmEmbeddingResponses>,
}

impl OwnedEmbeddingResponses {
    fn to_vec(&self) -> Result<Vec<Vec<f32>>> {
        // SAFETY: raw is a live batch responses pointer; the getter only reads from it.
        let size = unsafe { ffi::litert_lm_embedding_responses_get_size(self.raw.as_ptr()) };
        let mut out = Vec::with_capacity(size);
        for index in 0..size {
            // SAFETY: raw is live and index is within the reported bounds. The returned
            // response is borrowed from this container, so it is not deleted separately.
            let response =
                unsafe { ffi::litert_lm_embedding_responses_get_at(self.raw.as_ptr(), index) };
            if response.is_null() {
                return Err(Error::NullPointer("litert_lm_embedding_responses_get_at"));
            }
            out.push(response_values(response)?);
        }
        Ok(out)
    }
}

impl Drop for OwnedEmbeddingResponses {
    fn drop(&mut self) {
        // SAFETY: raw was returned by compute_embedding_batch and is owned here.
        unsafe { ffi::litert_lm_embedding_responses_delete(self.raw.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_document_prefixes_differ() {
        assert_ne!(
            EmbeddingTask::Query.prefix(),
            EmbeddingTask::Document.prefix()
        );
    }

    #[test]
    fn prompt_trims_surrounding_whitespace() {
        assert_eq!(
            EmbeddingTask::Query.prompt("  hello world  "),
            "task: search query | text: hello world"
        );
    }

    #[test]
    fn default_options_skip_native_allocation() {
        let options = RawEmbeddingOptions::new(EmbeddingOptions::default())
            .expect("default options need no native call");
        assert!(options.as_ptr().is_null());
    }

    #[test]
    fn non_default_options_allocate() {
        let options = RawEmbeddingOptions::new(EmbeddingOptions {
            normalize: Some(true),
            ..EmbeddingOptions::default()
        })
        .expect("options object is created");
        assert!(!options.as_ptr().is_null());
    }

    #[test]
    fn overflow_strategies_map_to_distinct_values() {
        let raw: Vec<_> = [
            InputOverflowStrategy::ChunkAndAverage,
            InputOverflowStrategy::Truncate,
            InputOverflowStrategy::Error,
        ]
        .into_iter()
        .map(InputOverflowStrategy::as_raw)
        .collect();
        assert_eq!(raw.len(), 3);
        assert_ne!(raw[0], raw[1]);
        assert_ne!(raw[1], raw[2]);
    }
}
