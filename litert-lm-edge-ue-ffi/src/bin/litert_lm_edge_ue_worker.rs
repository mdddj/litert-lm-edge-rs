use litert_lm_edge::{Backend, Engine, SessionConfig, StreamEvent};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};
use std::process;
use std::time::Duration;

const STATUS_OK: i32 = 0;
const STATUS_CANCELLED: i32 = 1;
const STATUS_INVALID_REQUEST: i32 = -1;
const STATUS_START_FAILED: i32 = -4;
const STATUS_STREAM_FAILED: i32 = -5;
const STATUS_PANIC: i32 = -100;

#[derive(Debug, Deserialize)]
struct WorkerRequest {
    model_path: String,
    prompt: String,
    #[serde(default = "default_backend")]
    backend: String,
    #[serde(default)]
    max_output_tokens: i32,
    #[serde(default)]
    cancel_file: Option<String>,
}

fn default_backend() -> String {
    "cpu".to_owned()
}

fn main() {
    let code = match panic::catch_unwind(AssertUnwindSafe(run)) {
        Ok(code) => code,
        Err(_) => {
            emit_error(STATUS_PANIC, "panic in LiteRT-LM worker");
            STATUS_PANIC
        }
    };
    emit_done(code);
    process::exit(if code == STATUS_OK || code == STATUS_CANCELLED {
        0
    } else {
        1
    });
}

fn run() -> i32 {
    let Some(request_path) = env::args().nth(1) else {
        emit_error(STATUS_INVALID_REQUEST, "missing request json path");
        return STATUS_INVALID_REQUEST;
    };
    let request_path = request_path.trim_matches('"');

    let request = match read_request(request_path) {
        Ok(request) => request,
        Err(error) => {
            emit_error(STATUS_INVALID_REQUEST, &error);
            return STATUS_INVALID_REQUEST;
        }
    };

    if request.model_path.is_empty() {
        emit_error(STATUS_INVALID_REQUEST, "model_path must not be empty");
        return STATUS_INVALID_REQUEST;
    }
    if request.prompt.is_empty() {
        emit_error(STATUS_INVALID_REQUEST, "prompt must not be empty");
        return STATUS_INVALID_REQUEST;
    }

    let backend = match request.backend.as_str() {
        "" | "cpu" => Backend::Cpu,
        "gpu" => Backend::Gpu,
        value => Backend::Custom(value.to_owned()),
    };

    let engine = match Engine::builder(&request.model_path)
        .backend(backend)
        .build()
    {
        Ok(engine) => engine,
        Err(error) => {
            emit_error(
                STATUS_START_FAILED,
                &format!("failed to create engine: {error}"),
            );
            return STATUS_START_FAILED;
        }
    };

    let config = SessionConfig {
        max_output_tokens: (request.max_output_tokens > 0).then_some(request.max_output_tokens),
        ..SessionConfig::default()
    };
    let mut session = match engine.create_session(config) {
        Ok(session) => session,
        Err(error) => {
            emit_error(
                STATUS_START_FAILED,
                &format!("failed to create session: {error}"),
            );
            return STATUS_START_FAILED;
        }
    };

    let mut stream = match session.generate_text_stream(&request.prompt) {
        Ok(stream) => stream,
        Err(error) => {
            emit_error(
                STATUS_START_FAILED,
                &format!("failed to start stream: {error}"),
            );
            return STATUS_START_FAILED;
        }
    };

    loop {
        let event = match stream.recv_timeout(Duration::from_millis(50)) {
            Ok(Some(event)) => event,
            Ok(None) => {
                if cancellation_requested(request.cancel_file.as_deref()) {
                    return STATUS_CANCELLED;
                }
                continue;
            }
            Err(error) => {
                emit_error(STATUS_STREAM_FAILED, &error.to_string());
                return STATUS_STREAM_FAILED;
            }
        };

        match event {
            StreamEvent::Chunk(chunk) => emit_token(&chunk),
            StreamEvent::Final => return STATUS_OK,
            StreamEvent::Error(error) => {
                emit_error(STATUS_STREAM_FAILED, &error);
                return STATUS_STREAM_FAILED;
            }
        }
    }
}

fn read_request(path: &str) -> Result<WorkerRequest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read request json at {path}: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("invalid request json: {error}"))
}

fn cancellation_requested(path: Option<&str>) -> bool {
    path.and_then(|path| fs::metadata(path).ok()).is_some()
}

fn emit_token(text: &str) {
    emit_line("TOKEN", 0, text);
}

fn emit_error(code: i32, text: &str) {
    emit_line("ERROR", code, text);
}

fn emit_done(code: i32) {
    emit_line("DONE", code, "");
}

fn emit_line(kind: &str, code: i32, text: &str) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{kind}\t{code}\t{}", hex_encode(text.as_bytes()));
    let _ = stdout.flush();
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
