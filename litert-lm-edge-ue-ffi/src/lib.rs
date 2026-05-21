use litert_lm_edge::{Backend, Engine, SessionConfig, StreamEvent};
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
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
const STATUS_PANIC: i32 = -100;

pub struct LiteRtLmEdgeStreamHandle {
    _private: [u8; 0],
}

struct StreamHandleInner {
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

pub type LiteRtLmEdgeTokenCallback =
    Option<extern "C" fn(user_data: *mut c_void, data: *const c_char, len: usize)>;
pub type LiteRtLmEdgeErrorCallback =
    Option<extern "C" fn(user_data: *mut c_void, code: i32, data: *const c_char, len: usize)>;
pub type LiteRtLmEdgeDoneCallback = Option<extern "C" fn(user_data: *mut c_void, code: i32)>;

struct StreamRequest {
    model_path: String,
    prompt: String,
    backend: Backend,
    max_output_tokens: Option<i32>,
}

struct StreamCallbacks {
    user_data: usize,
    on_token: LiteRtLmEdgeTokenCallback,
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
    match panic::catch_unwind(AssertUnwindSafe(|| {
        let callbacks = StreamCallbacks {
            user_data: user_data as usize,
            on_token,
            on_error,
            on_done,
        };
        let request = match parse_request(model_path, prompt, backend, max_output_tokens) {
            Ok(request) => request,
            Err((code, message)) => {
                callbacks.emit_error(code, &message);
                callbacks.emit_done(code);
                return ptr::null_mut();
            }
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::spawn(move || {
            let code = match panic::catch_unwind(AssertUnwindSafe(|| {
                run_stream(request, worker_cancel, &callbacks)
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
    })) {
        Ok(handle) => handle,
        Err(_) => {
            let callbacks = StreamCallbacks {
                user_data: user_data as usize,
                on_token,
                on_error,
                on_done,
            };
            callbacks.emit_error(STATUS_PANIC, "panic in LiteRT-LM FFI boundary");
            callbacks.emit_done(STATUS_PANIC);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_stream_cancel(handle: *mut LiteRtLmEdgeStreamHandle) {
    if handle.is_null() {
        return;
    }

    // SAFETY: handle is checked for null and points to a StreamHandleInner allocated by
    // litert_lm_edge_ue_stream_text_start until litert_lm_edge_ue_stream_free consumes it.
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
    // free takes ownership.
    let mut handle = unsafe { Box::from_raw(handle.cast::<StreamHandleInner>()) };
    handle.cancel.store(true, Ordering::Relaxed);
    if let Some(worker) = handle.worker.take() {
        let _ = worker.join();
    }
}

#[no_mangle]
pub extern "C" fn litert_lm_edge_ue_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

fn parse_request(
    model_path: *const c_char,
    prompt: *const c_char,
    backend: *const c_char,
    max_output_tokens: i32,
) -> Result<StreamRequest, (i32, String)> {
    let model_path = read_required_string(model_path, "model_path")?;
    let prompt = read_required_string(prompt, "prompt")?;
    let backend = read_optional_string(backend)?.unwrap_or_else(|| "cpu".to_owned());
    let backend = match backend.as_str() {
        "" | "cpu" => Backend::Cpu,
        "gpu" => Backend::Gpu,
        value => Backend::Custom(value.to_owned()),
    };
    let max_output_tokens = (max_output_tokens > 0).then_some(max_output_tokens);

    Ok(StreamRequest {
        model_path,
        prompt,
        backend,
        max_output_tokens,
    })
}

fn read_required_string(ptr: *const c_char, name: &str) -> Result<String, (i32, String)> {
    if ptr.is_null() {
        return Err((
            STATUS_NULL_ARGUMENT,
            format!("{name} pointer must not be null"),
        ));
    }

    // SAFETY: ptr is provided by the C caller and is expected to point to a NUL-terminated string.
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|err| {
            (
                STATUS_INVALID_UTF8,
                format!("{name} is not valid UTF-8: {err}"),
            )
        })?
        .to_owned();

    if value.is_empty() {
        return Err((STATUS_EMPTY_ARGUMENT, format!("{name} must not be empty")));
    }

    Ok(value)
}

fn read_optional_string(ptr: *const c_char) -> Result<Option<String>, (i32, String)> {
    if ptr.is_null() {
        return Ok(None);
    }

    // SAFETY: ptr is provided by the C caller and is expected to point to a NUL-terminated string.
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|err| {
            (
                STATUS_INVALID_UTF8,
                format!("backend is not valid UTF-8: {err}"),
            )
        })?
        .to_owned();
    Ok(Some(value))
}

fn run_stream(request: StreamRequest, cancel: Arc<AtomicBool>, callbacks: &StreamCallbacks) -> i32 {
    if cancel.load(Ordering::Relaxed) {
        return STATUS_CANCELLED;
    }

    let engine = match Engine::builder(&request.model_path)
        .backend(request.backend)
        .build()
    {
        Ok(engine) => engine,
        Err(error) => {
            callbacks.emit_error(
                STATUS_START_FAILED,
                &format!("failed to create engine: {error}"),
            );
            return STATUS_START_FAILED;
        }
    };

    let config = SessionConfig {
        max_output_tokens: request.max_output_tokens,
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

    let mut stream = match session.generate_text_stream(&request.prompt) {
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

impl StreamCallbacks {
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
        errors: Vec<(i32, String)>,
        done: Vec<i32>,
    }

    extern "C" fn capture_token(user_data: *mut c_void, data: *const c_char, len: usize) {
        let value = copy_callback_string(data, len);
        capture(user_data).tokens.push(value);
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

    #[test]
    fn parse_request_defaults_to_cpu() {
        let model = CString::new("/tmp/model.litertlm").unwrap();
        let prompt = CString::new("hello").unwrap();
        let request = parse_request(model.as_ptr(), prompt.as_ptr(), ptr::null(), 0).unwrap();
        assert_eq!(request.model_path, "/tmp/model.litertlm");
        assert_eq!(request.prompt, "hello");
        assert_eq!(request.max_output_tokens, None);
        assert!(matches!(request.backend, Backend::Cpu));
    }

    #[test]
    fn parse_request_accepts_custom_backend_and_tokens() {
        let model = CString::new("/tmp/model.litertlm").unwrap();
        let prompt = CString::new("hello").unwrap();
        let backend = CString::new("npu").unwrap();
        let request =
            parse_request(model.as_ptr(), prompt.as_ptr(), backend.as_ptr(), 128).unwrap();
        assert_eq!(request.max_output_tokens, Some(128));
        assert!(matches!(request.backend, Backend::Custom(value) if value == "npu"));
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
    fn cancel_and_free_are_null_safe() {
        litert_lm_edge_ue_stream_cancel(ptr::null_mut());
        litert_lm_edge_ue_stream_free(ptr::null_mut());
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
