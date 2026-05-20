use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("string contains an interior NUL byte")]
    Nul(#[from] std::ffi::NulError),

    #[error("LiteRT-LM returned a null pointer from {0}")]
    NullPointer(&'static str),

    #[error("LiteRT-LM failed to start stream with code {0}")]
    StartStream(i32),

    #[error("LiteRT-LM returned non-UTF-8 text")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("model path is not valid UTF-8")]
    ModelPath,

    #[error("LiteRT-LM library error: {0}")]
    Library(String),

    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "model-download")]
    #[error("HTTP error: {0}")]
    Http(#[from] Box<ureq::Error>),

    #[error("JSON error")]
    Json(#[from] serde_json::Error),

    #[error("invalid LiteRT-LM response: {0}")]
    InvalidResponse(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("tool error in {name}: {message}")]
    ToolError { name: String, message: String },

    #[error("conversation exceeded recurring tool call limit of {0}")]
    RecurringToolCallLimit(usize),

    #[cfg(feature = "tokio")]
    #[error("async LiteRT-LM worker stopped")]
    WorkerStopped,

    #[cfg(feature = "model-download")]
    #[error("downloaded model checksum mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}
