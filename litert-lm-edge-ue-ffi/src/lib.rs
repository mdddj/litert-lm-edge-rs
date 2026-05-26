use litert_lm_edge::{
    Backend, Content, Conversation, ConversationConfig, Engine, InputData, Message, SendOptions,
    SessionConfig, StreamEvent,
};
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const STATUS_OK: i32 = 0;
const STATUS_CANCELLED: i32 = 1;
const STATUS_NULL_ARGUMENT: i32 = -1;
const STATUS_INVALID_UTF8: i32 = -2;
const STATUS_EMPTY_ARGUMENT: i32 = -3;
const STATUS_START_FAILED: i32 = -4;
const STATUS_STREAM_FAILED: i32 = -5;
const STATUS_INVALID_JSON: i32 = -6;
const STATUS_BUSY: i32 = -7;
const STATUS_PANIC: i32 = -100;

pub struct LiteRtLmEdgeStreamHandle {
    _private: [u8; 0],
}

pub struct LiteRtLmEdgeConversationHandle {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LiteRtLmEdgeStringArray {
    pub data: *const *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LiteRtLmEdgeMultimodalRequest {
    pub model_path: *const c_char,
    pub prompt: *const c_char,
    pub image_paths: LiteRtLmEdgeStringArray,
    pub audio_paths: LiteRtLmEdgeStringArray,
    pub backend: *const c_char,
    pub vision_backend: *const c_char,
    pub audio_backend: *const c_char,
    pub max_num_images: i32,
    pub max_output_tokens: i32,
}

struct StreamHandleInner {
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct ConversationHandleInner {
    sender: mpsc::Sender<ConversationCommand>,
    cancel: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct ConversationState {
    conversation: Conversation<'static>,
    _engine: Engine,
}

enum ConversationCommand {
    SendMessage {
        message: Value,
        options: SendOptions,
        stream_cancel: Arc<AtomicBool>,
        callbacks: ConversationCallbacks,
        done: mpsc::Sender<i32>,
    },
    Cancel,
    Shutdown,
}

pub type LiteRtLmEdgeTokenCallback =
    Option<extern "C" fn(user_data: *mut c_void, data: *const c_char, len: usize)>;
pub type LiteRtLmEdgeErrorCallback =
    Option<extern "C" fn(user_data: *mut c_void, code: i32, data: *const c_char, len: usize)>;
pub type LiteRtLmEdgeDoneCallback = Option<extern "C" fn(user_data: *mut c_void, code: i32)>;
pub type LiteRtLmEdgeJsonCallback =
    Option<extern "C" fn(user_data: *mut c_void, data: *const c_char, len: usize)>;

struct StreamRequest {
    model_path: String,
    prompt: String,
    image_paths: Vec<String>,
    audio_paths: Vec<String>,
    backend: Backend,
    vision_backend: Option<Backend>,
    audio_backend: Option<Backend>,
    max_num_images: Option<i32>,
    max_output_tokens: Option<i32>,
}

struct ConversationRequest {
    model_path: String,
    backend: Backend,
    vision_backend: Option<Backend>,
    audio_backend: Option<Backend>,
    max_num_images: Option<i32>,
    max_output_tokens: Option<i32>,
    system_prompt: Option<String>,
    tools_json: Option<Value>,
}

#[derive(Clone, Copy)]
struct StreamCallbacks {
    user_data: usize,
    on_token: LiteRtLmEdgeTokenCallback,
    on_error: LiteRtLmEdgeErrorCallback,
    on_done: LiteRtLmEdgeDoneCallback,
}

#[derive(Clone, Copy)]
struct ConversationCallbacks {
    user_data: usize,
    on_text: LiteRtLmEdgeTokenCallback,
    on_tool_calls: LiteRtLmEdgeJsonCallback,
    on_response_json: LiteRtLmEdgeJsonCallback,
    on_error: LiteRtLmEdgeErrorCallback,
    on_done: LiteRtLmEdgeDoneCallback,
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_stream_text_start(
    model_path: *const c_char,
    prompt: *const c_char,
    backend: *const c_char,
    max_output_tokens: i32,
    user_data: *mut c_void,
    on_token: LiteRtLmEdgeTokenCallback,
    on_error: LiteRtLmEdgeErrorCallback,
    on_done: LiteRtLmEdgeDoneCallback,
) -> *mut LiteRtLmEdgeStreamHandle {
    let callbacks = StreamCallbacks::new(user_data, on_token, on_error, on_done);
    ffi_boundary(callbacks, || {
        let request = parse_text_stream_request(model_path, prompt, backend, max_output_tokens)?;
        Ok(start_stream_worker(request, callbacks))
    })
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_stream_multimodal_start(
    request: *const LiteRtLmEdgeMultimodalRequest,
    user_data: *mut c_void,
    on_token: LiteRtLmEdgeTokenCallback,
    on_error: LiteRtLmEdgeErrorCallback,
    on_done: LiteRtLmEdgeDoneCallback,
) -> *mut LiteRtLmEdgeStreamHandle {
    let callbacks = StreamCallbacks::new(user_data, on_token, on_error, on_done);
    ffi_boundary(callbacks, || {
        let request = parse_multimodal_stream_request(request)?;
        Ok(start_stream_worker(request, callbacks))
    })
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_stream_cancel(handle: *mut LiteRtLmEdgeStreamHandle) {
    if handle.is_null() {
        return;
    }

    // SAFETY: handle is checked for null and points to a StreamHandleInner allocated by this
    // crate until litert_lm_edge_ue_stream_free consumes it.
    unsafe {
        (*(handle.cast::<StreamHandleInner>()))
            .cancel
            .store(true, Ordering::Relaxed)
    };
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_stream_free(handle: *mut LiteRtLmEdgeStreamHandle) {
    if handle.is_null() {
        return;
    }

    // SAFETY: the handle was allocated as StreamHandleInner with Box::into_raw by this crate and
    // free takes ownership exactly once.
    let mut handle = unsafe { Box::from_raw(handle.cast::<StreamHandleInner>()) };
    handle.cancel.store(true, Ordering::Relaxed);
    if let Some(worker) = handle.worker.take() {
        let _ = worker.join();
    }
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_conversation_create(
    model_path: *const c_char,
    backend: *const c_char,
    vision_backend: *const c_char,
    audio_backend: *const c_char,
    max_num_images: i32,
    max_output_tokens: i32,
    system_prompt: *const c_char,
    tools_json: *const c_char,
    out_handle: *mut *mut LiteRtLmEdgeConversationHandle,
    user_data: *mut c_void,
    on_error: LiteRtLmEdgeErrorCallback,
) -> i32 {
    let callbacks = ConversationCallbacks::error_only(user_data, on_error);
    match panic::catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            callbacks.emit_error(STATUS_NULL_ARGUMENT, "out_handle pointer must not be null");
            return STATUS_NULL_ARGUMENT;
        }

        // SAFETY: out_handle is non-null and owned by the caller for this synchronous call.
        unsafe { *out_handle = ptr::null_mut() };

        let request = match parse_conversation_request(
            model_path,
            backend,
            vision_backend,
            audio_backend,
            max_num_images,
            max_output_tokens,
            system_prompt,
            tools_json,
        ) {
            Ok(request) => request,
            Err((code, message)) => {
                callbacks.emit_error(code, &message);
                return code;
            }
        };

        let handle = match create_conversation_handle(request) {
            Ok(handle) => handle,
            Err((code, message)) => {
                callbacks.emit_error(code, &message);
                return code;
            }
        };

        // SAFETY: out_handle is non-null and remains valid for this synchronous call.
        unsafe { *out_handle = handle };
        STATUS_OK
    })) {
        Ok(code) => code,
        Err(_) => {
            callbacks.emit_error(
                STATUS_PANIC,
                "panic in LiteRT-LM conversation create boundary",
            );
            STATUS_PANIC
        }
    }
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_conversation_cancel(
    handle: *mut LiteRtLmEdgeConversationHandle,
) {
    if handle.is_null() {
        return;
    }

    // SAFETY: handle is checked for null and points to a ConversationHandleInner allocated by this
    // crate until litert_lm_edge_ue_conversation_free consumes it.
    let inner = unsafe { &*(handle.cast::<ConversationHandleInner>()) };
    inner.cancel.store(true, Ordering::Relaxed);
    let _ = inner.sender.send(ConversationCommand::Cancel);
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_conversation_free(handle: *mut LiteRtLmEdgeConversationHandle) {
    if handle.is_null() {
        return;
    }

    litert_lm_edge_ue_conversation_cancel(handle);
    // SAFETY: the handle was allocated as ConversationHandleInner with Box::into_raw by this
    // crate and free takes ownership exactly once.
    let mut inner = unsafe { Box::from_raw(handle.cast::<ConversationHandleInner>()) };
    let _ = inner.sender.send(ConversationCommand::Shutdown);
    if let Some(worker) = inner.worker.take() {
        let _ = worker.join();
    }
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_conversation_send_message_start(
    handle: *mut LiteRtLmEdgeConversationHandle,
    prompt: *const c_char,
    image_paths: LiteRtLmEdgeStringArray,
    audio_paths: LiteRtLmEdgeStringArray,
    visual_token_budget: i32,
    user_data: *mut c_void,
    on_text: LiteRtLmEdgeTokenCallback,
    on_tool_calls: LiteRtLmEdgeJsonCallback,
    on_response_json: LiteRtLmEdgeJsonCallback,
    on_error: LiteRtLmEdgeErrorCallback,
    on_done: LiteRtLmEdgeDoneCallback,
) -> *mut LiteRtLmEdgeStreamHandle {
    let callbacks = ConversationCallbacks::new(
        user_data,
        on_text,
        on_tool_calls,
        on_response_json,
        on_error,
        on_done,
    );
    conversation_ffi_boundary(callbacks, || {
        if handle.is_null() {
            return Err((
                STATUS_NULL_ARGUMENT,
                "conversation handle pointer must not be null".to_owned(),
            ));
        }

        let prompt = read_required_string(prompt, "prompt")?;
        let image_paths = read_string_array(image_paths, "image_paths")?;
        let audio_paths = read_string_array(audio_paths, "audio_paths")?;
        let message = message_from_parts(prompt, image_paths, audio_paths);
        let options = SendOptions {
            visual_token_budget: (visual_token_budget > 0).then_some(visual_token_budget),
            automatic_tool_calling: false,
            ..SendOptions::default()
        };

        // SAFETY: handle is checked for null and points to a valid ConversationHandleInner until
        // the matching free function consumes it.
        let inner = unsafe { &*(handle.cast::<ConversationHandleInner>()) };
        Ok(start_conversation_request(
            inner.sender.clone(),
            Arc::clone(&inner.cancel),
            Arc::clone(&inner.busy),
            message.to_json(),
            options,
            callbacks,
        ))
    })
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_conversation_continue_tools_start(
    handle: *mut LiteRtLmEdgeConversationHandle,
    tool_results_json: *const c_char,
    user_data: *mut c_void,
    on_text: LiteRtLmEdgeTokenCallback,
    on_tool_calls: LiteRtLmEdgeJsonCallback,
    on_response_json: LiteRtLmEdgeJsonCallback,
    on_error: LiteRtLmEdgeErrorCallback,
    on_done: LiteRtLmEdgeDoneCallback,
) -> *mut LiteRtLmEdgeStreamHandle {
    let callbacks = ConversationCallbacks::new(
        user_data,
        on_text,
        on_tool_calls,
        on_response_json,
        on_error,
        on_done,
    );
    conversation_ffi_boundary(callbacks, || {
        if handle.is_null() {
            return Err((
                STATUS_NULL_ARGUMENT,
                "conversation handle pointer must not be null".to_owned(),
            ));
        }

        let tool_results_json = read_required_string(tool_results_json, "tool_results_json")?;
        let message = tool_results_message_json(&tool_results_json)?;
        let options = SendOptions {
            automatic_tool_calling: false,
            ..SendOptions::default()
        };

        // SAFETY: handle is checked for null and points to a valid ConversationHandleInner until
        // the matching free function consumes it.
        let inner = unsafe { &*(handle.cast::<ConversationHandleInner>()) };
        Ok(start_conversation_request(
            inner.sender.clone(),
            Arc::clone(&inner.cancel),
            Arc::clone(&inner.busy),
            message,
            options,
            callbacks,
        ))
    })
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

fn ffi_boundary(
    callbacks: StreamCallbacks,
    f: impl FnOnce() -> Result<*mut LiteRtLmEdgeStreamHandle, (i32, String)>,
) -> *mut LiteRtLmEdgeStreamHandle {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(handle)) => handle,
        Ok(Err((code, message))) => {
            callbacks.emit_error(code, &message);
            callbacks.emit_done(code);
            ptr::null_mut()
        }
        Err(_) => {
            callbacks.emit_error(STATUS_PANIC, "panic in LiteRT-LM FFI boundary");
            callbacks.emit_done(STATUS_PANIC);
            ptr::null_mut()
        }
    }
}

fn conversation_ffi_boundary(
    callbacks: ConversationCallbacks,
    f: impl FnOnce() -> Result<*mut LiteRtLmEdgeStreamHandle, (i32, String)>,
) -> *mut LiteRtLmEdgeStreamHandle {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(handle)) => handle,
        Ok(Err((code, message))) => {
            callbacks.emit_error(code, &message);
            callbacks.emit_done(code);
            ptr::null_mut()
        }
        Err(_) => {
            callbacks.emit_error(STATUS_PANIC, "panic in LiteRT-LM conversation FFI boundary");
            callbacks.emit_done(STATUS_PANIC);
            ptr::null_mut()
        }
    }
}

fn start_stream_worker(
    request: StreamRequest,
    callbacks: StreamCallbacks,
) -> *mut LiteRtLmEdgeStreamHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let worker = thread::spawn(move || {
        let code = match panic::catch_unwind(AssertUnwindSafe(|| {
            run_stream(request, worker_cancel, callbacks)
        })) {
            Ok(code) => code,
            Err(_) => {
                callbacks.emit_error(STATUS_PANIC, "panic in LiteRT-LM stream worker");
                STATUS_PANIC
            }
        };
        callbacks.emit_done(code);
    });

    let handle = Box::new(StreamHandleInner {
        cancel,
        worker: Some(worker),
    });
    Box::into_raw(handle).cast()
}

fn start_conversation_request(
    sender: mpsc::Sender<ConversationCommand>,
    shared_cancel: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    message: Value,
    options: SendOptions,
    callbacks: ConversationCallbacks,
) -> *mut LiteRtLmEdgeStreamHandle {
    if busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        callbacks.emit_error(STATUS_BUSY, "conversation is already processing a request");
        callbacks.emit_done(STATUS_BUSY);
        return ptr::null_mut();
    }

    shared_cancel.store(false, Ordering::Relaxed);
    let stream_cancel = Arc::new(AtomicBool::new(false));
    let request_cancel = Arc::clone(&stream_cancel);
    let busy_for_worker = Arc::clone(&busy);
    let worker = thread::spawn(move || {
        let (done_tx, done_rx) = mpsc::channel();
        let send_result = sender.send(ConversationCommand::SendMessage {
            message,
            options,
            stream_cancel: request_cancel,
            callbacks,
            done: done_tx,
        });

        let code = match send_result {
            Ok(()) => done_rx.recv().unwrap_or(STATUS_STREAM_FAILED),
            Err(_) => {
                callbacks.emit_error(STATUS_STREAM_FAILED, "conversation worker stopped");
                STATUS_STREAM_FAILED
            }
        };
        busy_for_worker.store(false, Ordering::Release);
        callbacks.emit_done(code);
    });

    let handle = Box::new(StreamHandleInner {
        cancel: stream_cancel,
        worker: Some(worker),
    });
    Box::into_raw(handle).cast()
}

fn parse_text_stream_request(
    model_path: *const c_char,
    prompt: *const c_char,
    backend: *const c_char,
    max_output_tokens: i32,
) -> Result<StreamRequest, (i32, String)> {
    let model_path = read_required_string(model_path, "model_path")?;
    let prompt = read_required_string(prompt, "prompt")?;
    let backend = parse_backend(backend, "backend")?.unwrap_or(Backend::Cpu);

    Ok(StreamRequest {
        model_path,
        prompt,
        image_paths: Vec::new(),
        audio_paths: Vec::new(),
        backend,
        vision_backend: None,
        audio_backend: None,
        max_num_images: None,
        max_output_tokens: (max_output_tokens > 0).then_some(max_output_tokens),
    })
}

fn parse_multimodal_stream_request(
    request: *const LiteRtLmEdgeMultimodalRequest,
) -> Result<StreamRequest, (i32, String)> {
    if request.is_null() {
        return Err((
            STATUS_NULL_ARGUMENT,
            "multimodal request pointer must not be null".to_owned(),
        ));
    }

    // SAFETY: request is checked for null and is copied immediately, so the caller only needs to
    // keep it valid for this synchronous parse.
    let request = unsafe { *request };
    let model_path = read_required_string(request.model_path, "model_path")?;
    let prompt = read_required_string(request.prompt, "prompt")?;
    let image_paths = read_string_array(request.image_paths, "image_paths")?;
    let audio_paths = read_string_array(request.audio_paths, "audio_paths")?;
    let backend = parse_backend(request.backend, "backend")?.unwrap_or(Backend::Cpu);
    let vision_backend = parse_backend(request.vision_backend, "vision_backend")?;
    let audio_backend = parse_backend(request.audio_backend, "audio_backend")?;
    let max_num_images = (request.max_num_images > 0)
        .then_some(request.max_num_images)
        .or_else(|| (!image_paths.is_empty()).then_some(image_paths.len() as i32));

    Ok(StreamRequest {
        model_path,
        prompt,
        image_paths,
        audio_paths,
        backend,
        vision_backend,
        audio_backend,
        max_num_images,
        max_output_tokens: (request.max_output_tokens > 0).then_some(request.max_output_tokens),
    })
}

fn parse_conversation_request(
    model_path: *const c_char,
    backend: *const c_char,
    vision_backend: *const c_char,
    audio_backend: *const c_char,
    max_num_images: i32,
    max_output_tokens: i32,
    system_prompt: *const c_char,
    tools_json: *const c_char,
) -> Result<ConversationRequest, (i32, String)> {
    let model_path = read_required_string(model_path, "model_path")?;
    let backend = parse_backend(backend, "backend")?.unwrap_or(Backend::Cpu);
    let vision_backend = parse_backend(vision_backend, "vision_backend")?;
    let audio_backend = parse_backend(audio_backend, "audio_backend")?;
    let system_prompt = read_optional_non_empty_string(system_prompt, "system_prompt")?;
    let tools_json = read_optional_non_empty_string(tools_json, "tools_json")?
        .map(|text| serde_json::from_str(&text))
        .transpose()
        .map_err(|err| {
            (
                STATUS_INVALID_JSON,
                format!("tools_json is not valid JSON: {err}"),
            )
        })?;

    Ok(ConversationRequest {
        model_path,
        backend,
        vision_backend,
        audio_backend,
        max_num_images: (max_num_images > 0).then_some(max_num_images),
        max_output_tokens: (max_output_tokens > 0).then_some(max_output_tokens),
        system_prompt,
        tools_json,
    })
}

fn create_conversation_handle(
    request: ConversationRequest,
) -> Result<*mut LiteRtLmEdgeConversationHandle, (i32, String)> {
    let (sender, receiver) = mpsc::channel();
    let (initialized_tx, initialized_rx) = mpsc::channel();
    let worker = thread::spawn(move || match build_conversation_state(request) {
        Ok(state) => {
            let _ = initialized_tx.send(Ok(()));
            conversation_worker_loop(state, receiver);
        }
        Err(error) => {
            let _ = initialized_tx.send(Err(error));
        }
    });

    match initialized_rx.recv().map_err(|_| {
        (
            STATUS_START_FAILED,
            "conversation worker stopped during startup".to_owned(),
        )
    })? {
        Ok(()) => {}
        Err(error) => {
            let _ = worker.join();
            return Err(error);
        }
    }

    let handle = Box::new(ConversationHandleInner {
        sender,
        cancel: Arc::new(AtomicBool::new(false)),
        busy: Arc::new(AtomicBool::new(false)),
        worker: Some(worker),
    });
    Ok(Box::into_raw(handle).cast())
}

fn build_conversation_state(
    request: ConversationRequest,
) -> Result<ConversationState, (i32, String)> {
    let engine = build_engine(
        &request.model_path,
        request.backend,
        request.vision_backend,
        request.audio_backend,
        request.max_num_images,
    )?;

    let mut config = ConversationConfig::new().session_config(SessionConfig {
        max_output_tokens: request.max_output_tokens,
        ..SessionConfig::default()
    });
    if let Some(system_prompt) = request.system_prompt {
        config = config.system_message(Message::system(system_prompt));
    }
    if let Some(tools_json) = request.tools_json {
        config = config.tools_json(tools_json);
    }

    let conversation = engine.create_conversation(config).map_err(|error| {
        (
            STATUS_START_FAILED,
            format!("failed to create conversation: {error}"),
        )
    })?;
    // SAFETY: ConversationHandleInner stores engine and conversation together and drops
    // conversation before engine because fields in ConversationState are declared conversation
    // first, engine second. The transmuted lifetime never leaves the worker thread.
    let conversation =
        unsafe { std::mem::transmute::<Conversation<'_>, Conversation<'static>>(conversation) };
    Ok(ConversationState {
        conversation,
        _engine: engine,
    })
}

fn read_required_string(ptr: *const c_char, name: &str) -> Result<String, (i32, String)> {
    let value = read_optional_string(ptr, name)?.ok_or_else(|| {
        (
            STATUS_NULL_ARGUMENT,
            format!("{name} pointer must not be null"),
        )
    })?;

    if value.is_empty() {
        return Err((STATUS_EMPTY_ARGUMENT, format!("{name} must not be empty")));
    }

    Ok(value)
}

fn read_optional_non_empty_string(
    ptr: *const c_char,
    name: &str,
) -> Result<Option<String>, (i32, String)> {
    let value = read_optional_string(ptr, name)?;
    Ok(value.and_then(|text| (!text.is_empty()).then_some(text)))
}

fn read_optional_string(ptr: *const c_char, name: &str) -> Result<Option<String>, (i32, String)> {
    if ptr.is_null() {
        return Ok(None);
    }

    // SAFETY: ptr is provided by the C caller and is expected to point to a NUL-terminated string
    // valid for this synchronous read.
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|err| {
            (
                STATUS_INVALID_UTF8,
                format!("{name} is not valid UTF-8: {err}"),
            )
        })?
        .to_owned();
    Ok(Some(value))
}

fn read_string_array(
    array: LiteRtLmEdgeStringArray,
    name: &str,
) -> Result<Vec<String>, (i32, String)> {
    if array.len == 0 {
        return Ok(Vec::new());
    }
    if array.data.is_null() {
        return Err((
            STATUS_NULL_ARGUMENT,
            format!("{name}.data pointer must not be null when len > 0"),
        ));
    }

    // SAFETY: array.data is non-null and the caller promises it points to len entries valid for
    // this synchronous parse. Each string is copied immediately.
    let items = unsafe { std::slice::from_raw_parts(array.data, array.len) };
    let mut values = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        values.push(read_required_string(*item, &format!("{name}[{index}]"))?);
    }
    Ok(values)
}

fn parse_backend(ptr: *const c_char, name: &str) -> Result<Option<Backend>, (i32, String)> {
    let value = read_optional_string(ptr, name)?;
    Ok(value.map(|value| match value.as_str() {
        "" | "cpu" => Backend::Cpu,
        "gpu" => Backend::Gpu,
        value => Backend::Custom(value.to_owned()),
    }))
}

fn build_engine(
    model_path: &str,
    backend: Backend,
    vision_backend: Option<Backend>,
    audio_backend: Option<Backend>,
    max_num_images: Option<i32>,
) -> Result<Engine, (i32, String)> {
    let mut builder = Engine::builder(model_path).backend(backend);
    if let Some(backend) = vision_backend {
        builder = builder.vision_backend(backend);
    }
    if let Some(backend) = audio_backend {
        builder = builder.audio_backend(backend);
    }
    if let Some(max_num_images) = max_num_images {
        builder = builder.max_num_images(max_num_images);
    }
    builder.build().map_err(|error| {
        (
            STATUS_START_FAILED,
            format!("failed to create engine: {error}"),
        )
    })
}

fn conversation_worker_loop(
    mut state: ConversationState,
    receiver: mpsc::Receiver<ConversationCommand>,
) {
    for command in receiver {
        match command {
            ConversationCommand::SendMessage {
                message,
                options,
                stream_cancel,
                callbacks,
                done,
            } => {
                let code = match panic::catch_unwind(AssertUnwindSafe(|| {
                    run_conversation_message(&mut state, stream_cancel, message, options, callbacks)
                })) {
                    Ok(code) => code,
                    Err(_) => {
                        callbacks
                            .emit_error(STATUS_PANIC, "panic in LiteRT-LM conversation worker");
                        STATUS_PANIC
                    }
                };
                let _ = done.send(code);
            }
            ConversationCommand::Cancel => state.conversation.cancel_process(),
            ConversationCommand::Shutdown => break,
        }
    }
}

fn run_stream(request: StreamRequest, cancel: Arc<AtomicBool>, callbacks: StreamCallbacks) -> i32 {
    if cancel.load(Ordering::Relaxed) {
        return STATUS_CANCELLED;
    }

    let engine = match build_engine(
        &request.model_path,
        request.backend.clone(),
        request.vision_backend.clone(),
        request.audio_backend.clone(),
        request.max_num_images,
    ) {
        Ok(engine) => engine,
        Err((code, message)) => {
            callbacks.emit_error(code, &message);
            return code;
        }
    };

    let config = SessionConfig {
        max_output_tokens: request.max_output_tokens,
        apply_prompt_template: Some(true),
        ..SessionConfig::default()
    };
    let mut session = match engine.create_session(config) {
        Ok(session) => session,
        Err(error) => {
            callbacks.emit_error(
                STATUS_START_FAILED,
                &format!("failed to create session: {error}"),
            );
            return STATUS_START_FAILED;
        }
    };

    let inputs = stream_inputs(&request);
    let mut stream = match session.generate_content_stream(&inputs) {
        Ok(stream) => stream,
        Err(error) => {
            callbacks.emit_error(
                STATUS_START_FAILED,
                &format!("failed to start stream: {error}"),
            );
            return STATUS_START_FAILED;
        }
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            return STATUS_CANCELLED;
        }

        let event = match stream.recv_timeout(Duration::from_millis(50)) {
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(error) => {
                callbacks.emit_error(STATUS_STREAM_FAILED, &error.to_string());
                return STATUS_STREAM_FAILED;
            }
        };

        match event {
            StreamEvent::Chunk(chunk) => callbacks.emit_token(&chunk),
            StreamEvent::Final => return STATUS_OK,
            StreamEvent::Error(error) => {
                callbacks.emit_error(STATUS_STREAM_FAILED, &error);
                return STATUS_STREAM_FAILED;
            }
        }
    }
}

fn run_conversation_message(
    state: &mut ConversationState,
    stream_cancel: Arc<AtomicBool>,
    message: Value,
    options: SendOptions,
    callbacks: ConversationCallbacks,
) -> i32 {
    if stream_cancel.load(Ordering::Relaxed) {
        return STATUS_CANCELLED;
    }

    let response = state.conversation.send_message_raw(message, options);

    if stream_cancel.load(Ordering::Relaxed) {
        return STATUS_CANCELLED;
    }

    match response {
        Ok(response) => {
            emit_conversation_response(&response, callbacks);
            STATUS_OK
        }
        Err(error) => {
            callbacks.emit_error(STATUS_STREAM_FAILED, &error.to_string());
            STATUS_STREAM_FAILED
        }
    }
}

fn stream_inputs(request: &StreamRequest) -> Vec<InputData> {
    let mut inputs = vec![InputData::text(request.prompt.clone())];
    if !request.image_paths.is_empty() {
        inputs.extend(request.image_paths.iter().map(InputData::image_file));
        inputs.push(InputData::ImageEnd);
    }
    if !request.audio_paths.is_empty() {
        inputs.extend(request.audio_paths.iter().map(InputData::audio_file));
        inputs.push(InputData::AudioEnd);
    }
    inputs
}

fn message_from_parts(
    prompt: String,
    image_paths: Vec<String>,
    audio_paths: Vec<String>,
) -> Message {
    let mut contents = vec![Content::text(prompt)];
    contents.extend(image_paths.iter().map(Content::image_file));
    contents.extend(audio_paths.iter().map(Content::audio_file));
    Message::user(contents)
}

fn tool_results_message_json(tool_results_json: &str) -> Result<Value, (i32, String)> {
    let value: Value = serde_json::from_str(tool_results_json).map_err(|err| {
        (
            STATUS_INVALID_JSON,
            format!("tool_results_json is not valid JSON: {err}"),
        )
    })?;
    let results = value.as_array().ok_or_else(|| {
        (
            STATUS_INVALID_JSON,
            "tool_results_json must be an array".to_owned(),
        )
    })?;

    let mut contents = Vec::with_capacity(results.len());
    for (index, item) in results.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                (
                    STATUS_INVALID_JSON,
                    format!("tool_results_json[{index}].name must be a string"),
                )
            })?
            .to_owned();
        let response = item.get("response").cloned().unwrap_or(Value::Null);
        contents.push(json!({
            "type": "tool_response",
            "name": name,
            "response": response,
        }));
    }

    Ok(json!({
        "role": "tool",
        "content": contents,
    }))
}

fn emit_conversation_response(response: &Value, callbacks: ConversationCallbacks) {
    let text = response_text(response);
    if !text.is_empty() {
        callbacks.emit_text(&text);
    }

    if let Some(tool_calls) = response.get("tool_calls").filter(|value| !value.is_null()) {
        callbacks.emit_tool_calls_value(tool_calls);
    }

    callbacks.emit_response_json_value(response);
}

fn response_text(response: &Value) -> String {
    let Some(contents) = response.get("content").and_then(Value::as_array) else {
        return String::new();
    };
    contents
        .iter()
        .filter_map(|content| content.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

impl StreamCallbacks {
    fn new(
        user_data: *mut c_void,
        on_token: LiteRtLmEdgeTokenCallback,
        on_error: LiteRtLmEdgeErrorCallback,
        on_done: LiteRtLmEdgeDoneCallback,
    ) -> Self {
        Self {
            user_data: user_data as usize,
            on_token,
            on_error,
            on_done,
        }
    }

    fn emit_token(&self, text: &str) {
        if let Some(callback) = self.on_token {
            emit_string(self.user_data, text, |user_data, data, len| {
                callback(user_data, data, len);
            });
        }
    }

    fn emit_error(&self, code: i32, text: &str) {
        if let Some(callback) = self.on_error {
            emit_string(self.user_data, text, |user_data, data, len| {
                callback(user_data, code, data, len);
            });
        }
    }

    fn emit_done(&self, code: i32) {
        if let Some(callback) = self.on_done {
            callback(self.user_data as *mut c_void, code);
        }
    }
}

impl ConversationCallbacks {
    fn new(
        user_data: *mut c_void,
        on_text: LiteRtLmEdgeTokenCallback,
        on_tool_calls: LiteRtLmEdgeJsonCallback,
        on_response_json: LiteRtLmEdgeJsonCallback,
        on_error: LiteRtLmEdgeErrorCallback,
        on_done: LiteRtLmEdgeDoneCallback,
    ) -> Self {
        Self {
            user_data: user_data as usize,
            on_text,
            on_tool_calls,
            on_response_json,
            on_error,
            on_done,
        }
    }

    fn error_only(user_data: *mut c_void, on_error: LiteRtLmEdgeErrorCallback) -> Self {
        Self::new(user_data, None, None, None, on_error, None)
    }

    fn emit_text(&self, text: &str) {
        if let Some(callback) = self.on_text {
            emit_string(self.user_data, text, |user_data, data, len| {
                callback(user_data, data, len);
            });
        }
    }

    fn emit_tool_calls_value(&self, value: &Value) {
        if let Some(callback) = self.on_tool_calls {
            let text = serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned());
            emit_string(self.user_data, &text, |user_data, data, len| {
                callback(user_data, data, len);
            });
        }
    }

    fn emit_response_json_value(&self, value: &Value) {
        if let Some(callback) = self.on_response_json {
            let text = serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned());
            emit_string(self.user_data, &text, |user_data, data, len| {
                callback(user_data, data, len);
            });
        }
    }

    fn emit_error(&self, code: i32, text: &str) {
        if let Some(callback) = self.on_error {
            emit_string(self.user_data, text, |user_data, data, len| {
                callback(user_data, code, data, len);
            });
        }
    }

    fn emit_done(&self, code: i32) {
        if let Some(callback) = self.on_done {
            callback(self.user_data as *mut c_void, code);
        }
    }
}

fn emit_string(
    user_data: usize,
    text: &str,
    callback: impl FnOnce(*mut c_void, *const c_char, usize),
) {
    let sanitized = text.replace('\0', "");
    let c_string = CString::new(sanitized).unwrap_or_else(|_| CString::new("").expect("empty"));
    let bytes = c_string.as_bytes();
    callback(user_data as *mut c_void, c_string.as_ptr(), bytes.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Captured {
        tokens: Vec<String>,
        json: Vec<String>,
        errors: Vec<(i32, String)>,
        done: Vec<i32>,
    }

    extern "C" fn capture_token(user_data: *mut c_void, data: *const c_char, len: usize) {
        let value = copy_callback_string(data, len);
        capture(user_data).tokens.push(value);
    }

    extern "C" fn capture_json(user_data: *mut c_void, data: *const c_char, len: usize) {
        let value = copy_callback_string(data, len);
        capture(user_data).json.push(value);
    }

    extern "C" fn capture_error(
        user_data: *mut c_void,
        code: i32,
        data: *const c_char,
        len: usize,
    ) {
        let value = copy_callback_string(data, len);
        capture(user_data).errors.push((code, value));
    }

    extern "C" fn capture_done(user_data: *mut c_void, code: i32) {
        capture(user_data).done.push(code);
    }

    fn copy_callback_string(data: *const c_char, len: usize) -> String {
        assert!(!data.is_null());
        // SAFETY: tests receive callback data from emit_string and copy it before callback returns.
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) };
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn capture(user_data: *mut c_void) -> &'static mut Captured {
        assert!(!user_data.is_null());
        // SAFETY: tests pass a unique Captured pointer as user_data for the duration of the
        // synchronous callback.
        unsafe { &mut *user_data.cast::<Captured>() }
    }

    fn string_array(items: &[CString]) -> (Vec<*const c_char>, LiteRtLmEdgeStringArray) {
        let ptrs = items.iter().map(|item| item.as_ptr()).collect::<Vec<_>>();
        let array = LiteRtLmEdgeStringArray {
            data: ptrs.as_ptr(),
            len: ptrs.len(),
        };
        (ptrs, array)
    }

    #[test]
    fn parse_text_request_defaults_to_cpu() {
        let model = CString::new("/tmp/model.litertlm").unwrap();
        let prompt = CString::new("hello").unwrap();
        let request =
            parse_text_stream_request(model.as_ptr(), prompt.as_ptr(), ptr::null(), 0).unwrap();
        assert_eq!(request.model_path, "/tmp/model.litertlm");
        assert_eq!(request.prompt, "hello");
        assert_eq!(request.max_output_tokens, None);
        assert!(matches!(request.backend, Backend::Cpu));
    }

    #[test]
    fn parse_multimodal_request_reads_paths_and_backends() {
        let model = CString::new("/tmp/model.litertlm").unwrap();
        let prompt = CString::new("describe").unwrap();
        let image = CString::new("/tmp/a.png").unwrap();
        let audio = CString::new("/tmp/a.wav").unwrap();
        let backend = CString::new("cpu").unwrap();
        let vision = CString::new("gpu").unwrap();
        let audio_backend = CString::new("npu").unwrap();
        let image_items = [image];
        let audio_items = [audio];
        let (_image_ptrs, image_paths) = string_array(&image_items);
        let (_audio_ptrs, audio_paths) = string_array(&audio_items);

        let raw = LiteRtLmEdgeMultimodalRequest {
            model_path: model.as_ptr(),
            prompt: prompt.as_ptr(),
            image_paths,
            audio_paths,
            backend: backend.as_ptr(),
            vision_backend: vision.as_ptr(),
            audio_backend: audio_backend.as_ptr(),
            max_num_images: 0,
            max_output_tokens: 128,
        };

        let request = parse_multimodal_stream_request(&raw).unwrap();
        assert_eq!(request.image_paths, vec!["/tmp/a.png"]);
        assert_eq!(request.audio_paths, vec!["/tmp/a.wav"]);
        assert_eq!(request.max_num_images, Some(1));
        assert_eq!(request.max_output_tokens, Some(128));
        assert!(matches!(request.vision_backend, Some(Backend::Gpu)));
        assert!(matches!(request.audio_backend, Some(Backend::Custom(value)) if value == "npu"));
    }

    #[test]
    fn stream_inputs_insert_modality_end_markers() {
        let request = StreamRequest {
            model_path: "/tmp/model.litertlm".to_owned(),
            prompt: "describe".to_owned(),
            image_paths: vec!["/tmp/a.png".to_owned(), "/tmp/b.png".to_owned()],
            audio_paths: vec!["/tmp/a.wav".to_owned()],
            backend: Backend::Cpu,
            vision_backend: None,
            audio_backend: None,
            max_num_images: None,
            max_output_tokens: None,
        };

        let inputs = stream_inputs(&request);
        assert!(matches!(inputs[0], InputData::Text(_)));
        assert!(matches!(inputs[1], InputData::ImageFile(_)));
        assert!(matches!(inputs[2], InputData::ImageFile(_)));
        assert!(matches!(inputs[3], InputData::ImageEnd));
        assert!(matches!(inputs[4], InputData::AudioFile(_)));
        assert!(matches!(inputs[5], InputData::AudioEnd));
    }

    #[test]
    fn null_model_returns_null_and_reports_error() {
        let mut captured = Captured::default();
        let prompt = CString::new("hello").unwrap();
        let handle = litert_lm_edge_ue_stream_text_start(
            ptr::null(),
            prompt.as_ptr(),
            ptr::null(),
            0,
            (&mut captured as *mut Captured).cast(),
            Some(capture_token),
            Some(capture_error),
            Some(capture_done),
        );
        assert!(handle.is_null());

        assert!(captured.tokens.is_empty());
        assert_eq!(captured.errors[0].0, STATUS_NULL_ARGUMENT);
        assert_eq!(captured.done, vec![STATUS_NULL_ARGUMENT]);
    }

    #[test]
    fn callbacks_copy_string_data() {
        let mut captured = Captured::default();
        let callbacks = StreamCallbacks {
            user_data: (&mut captured as *mut Captured) as usize,
            on_token: Some(capture_token),
            on_error: Some(capture_error),
            on_done: Some(capture_done),
        };

        callbacks.emit_token("a\0b");
        callbacks.emit_error(-42, "bad");
        callbacks.emit_done(STATUS_OK);

        assert_eq!(captured.tokens, vec!["ab"]);
        assert_eq!(captured.errors, vec![(-42, "bad".to_owned())]);
        assert_eq!(captured.done, vec![STATUS_OK]);
    }

    #[test]
    fn tool_results_json_converts_to_tool_message() {
        let message = tool_results_message_json(
            r#"[{"id":"call_1","name":"product","response":{"product":42}}]"#,
        )
        .unwrap();

        assert_eq!(
            message,
            json!({
                "role": "tool",
                "content": [{
                    "type": "tool_response",
                    "name": "product",
                    "response": {"product": 42}
                }]
            })
        );
    }

    #[test]
    fn conversation_callbacks_emit_tool_calls_and_response_json() {
        let mut captured = Captured::default();
        let callbacks = ConversationCallbacks::new(
            (&mut captured as *mut Captured).cast(),
            Some(capture_token),
            Some(capture_json),
            Some(capture_json),
            Some(capture_error),
            Some(capture_done),
        );
        let response = json!({
            "role": "model",
            "content": [{"type": "text", "text": "calling tool"}],
            "tool_calls": [{"function": {"name": "product", "arguments": {"numbers": [6, 7]}}}]
        });

        emit_conversation_response(&response, callbacks);
        callbacks.emit_done(STATUS_OK);

        assert_eq!(captured.tokens, vec!["calling tool"]);
        assert_eq!(captured.json.len(), 2);
        assert!(captured.json[0].contains("product"));
        assert!(captured.json[1].contains("tool_calls"));
        assert_eq!(captured.done, vec![STATUS_OK]);
    }

    #[test]
    fn cancel_and_free_are_null_safe() {
        litert_lm_edge_ue_stream_cancel(ptr::null_mut());
        litert_lm_edge_ue_stream_free(ptr::null_mut());
        litert_lm_edge_ue_conversation_cancel(ptr::null_mut());
        litert_lm_edge_ue_conversation_free(ptr::null_mut());
    }

    #[test]
    fn version_is_static_c_string() {
        let version = litert_lm_edge_ue_version();
        assert!(!version.is_null());
        // SAFETY: litert_lm_edge_ue_version returns a static NUL-terminated string.
        let version = unsafe { CStr::from_ptr(version) }.to_str().unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }
}
