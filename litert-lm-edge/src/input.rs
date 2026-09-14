use crate::{Error, Result};
use litert_lm_edge_sys as ffi;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputData {
    Text(String),
    ImageBytes(Vec<u8>),
    ImageFile(PathBuf),
    ImageEnd,
    AudioBytes(Vec<u8>),
    AudioFile(PathBuf),
    AudioEnd,
}

impl InputData {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    pub fn image_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::ImageBytes(bytes.into())
    }

    pub fn image_file(path: impl AsRef<Path>) -> Self {
        Self::ImageFile(path.as_ref().to_path_buf())
    }

    pub fn audio_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::AudioBytes(bytes.into())
    }

    pub fn audio_file(path: impl AsRef<Path>) -> Self {
        Self::AudioFile(path.as_ref().to_path_buf())
    }
}

pub(crate) struct OwnedInputs {
    ffi: Vec<*const ffi::LiteRtLmInputData>,
}

impl OwnedInputs {
    pub(crate) fn new(inputs: &[InputData]) -> Result<Self> {
        let mut owned = Self {
            ffi: Vec::with_capacity(inputs.len()),
        };
        for input in inputs {
            let file_bytes;
            let (kind, bytes): (_, &[u8]) = match input {
                InputData::Text(text) => (ffi::kLiteRtLmInputDataTypeText, text.as_bytes()),
                InputData::ImageBytes(bytes) => (ffi::kLiteRtLmInputDataTypeImage, bytes),
                InputData::AudioBytes(bytes) => (ffi::kLiteRtLmInputDataTypeAudio, bytes),
                InputData::ImageFile(path) => {
                    file_bytes = std::fs::read(path)?;
                    (ffi::kLiteRtLmInputDataTypeImage, &file_bytes)
                }
                InputData::AudioFile(path) => {
                    file_bytes = std::fs::read(path)?;
                    (ffi::kLiteRtLmInputDataTypeAudio, &file_bytes)
                }
                InputData::ImageEnd => (ffi::kLiteRtLmInputDataTypeImageEnd, &[]),
                InputData::AudioEnd => (ffi::kLiteRtLmInputDataTypeAudioEnd, &[]),
            };
            // SAFETY: the constructor copies bytes before returning; end markers ignore them.
            let raw = unsafe {
                ffi::litert_lm_input_data_create(kind, bytes.as_ptr().cast(), bytes.len())
            };
            if raw.is_null() {
                return Err(Error::NullPointer("litert_lm_input_data_create"));
            }
            owned.ffi.push(raw);
        }
        Ok(owned)
    }

    pub(crate) fn as_ffi(&self) -> &[*const ffi::LiteRtLmInputData] {
        &self.ffi
    }
}

impl Drop for OwnedInputs {
    fn drop(&mut self) {
        for &raw in &self.ffi {
            // SAFETY: each pointer is uniquely owned and was returned by input_data_create.
            unsafe { ffi::litert_lm_input_data_delete(raw.cast_mut()) };
        }
    }
}
