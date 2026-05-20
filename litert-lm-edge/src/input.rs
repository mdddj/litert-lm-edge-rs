use crate::Result;
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
    _buffers: Vec<Vec<u8>>,
    ffi: Vec<ffi::LiteRtLmInputData>,
}

impl OwnedInputs {
    pub(crate) fn new(inputs: &[InputData]) -> Result<Self> {
        let mut buffers = Vec::new();
        let mut ffi_inputs = Vec::with_capacity(inputs.len());

        for input in inputs {
            match input {
                InputData::Text(text) => {
                    buffers.push(text.as_bytes().to_vec());
                    push_buffer(&mut ffi_inputs, &buffers, ffi::kLiteRtLmInputDataTypeText);
                }
                InputData::ImageBytes(bytes) => {
                    buffers.push(bytes.clone());
                    push_buffer(&mut ffi_inputs, &buffers, ffi::kLiteRtLmInputDataTypeImage);
                }
                InputData::ImageFile(path) => {
                    buffers.push(std::fs::read(path)?);
                    push_buffer(&mut ffi_inputs, &buffers, ffi::kLiteRtLmInputDataTypeImage);
                }
                InputData::ImageEnd => {
                    ffi_inputs.push(end_marker(ffi::kLiteRtLmInputDataTypeImageEnd))
                }
                InputData::AudioBytes(bytes) => {
                    buffers.push(bytes.clone());
                    push_buffer(&mut ffi_inputs, &buffers, ffi::kLiteRtLmInputDataTypeAudio);
                }
                InputData::AudioFile(path) => {
                    buffers.push(std::fs::read(path)?);
                    push_buffer(&mut ffi_inputs, &buffers, ffi::kLiteRtLmInputDataTypeAudio);
                }
                InputData::AudioEnd => {
                    ffi_inputs.push(end_marker(ffi::kLiteRtLmInputDataTypeAudioEnd))
                }
            }
        }

        Ok(Self {
            _buffers: buffers,
            ffi: ffi_inputs,
        })
    }

    pub(crate) fn as_ffi(&self) -> &[ffi::LiteRtLmInputData] {
        &self.ffi
    }
}

fn push_buffer(
    ffi_inputs: &mut Vec<ffi::LiteRtLmInputData>,
    buffers: &[Vec<u8>],
    input_type: ffi::LiteRtLmInputDataType,
) {
    let buffer = buffers.last().expect("buffer was just pushed");
    ffi_inputs.push(ffi::LiteRtLmInputData {
        type_: input_type,
        data: buffer.as_ptr().cast(),
        size: buffer.len(),
    });
}

fn end_marker(input_type: ffi::LiteRtLmInputDataType) -> ffi::LiteRtLmInputData {
    ffi::LiteRtLmInputData {
        type_: input_type,
        data: std::ptr::null(),
        size: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_multimodal_inputs_to_ffi() {
        let inputs = OwnedInputs::new(&[
            InputData::Text("describe".to_owned()),
            InputData::ImageBytes(vec![1, 2, 3]),
            InputData::ImageEnd,
            InputData::AudioBytes(vec![4, 5]),
            InputData::AudioEnd,
        ])
        .unwrap();

        assert_eq!(inputs.as_ffi().len(), 5);
        assert_eq!(inputs.as_ffi()[0].type_, ffi::kLiteRtLmInputDataTypeText);
        assert_eq!(inputs.as_ffi()[0].size, 8);
        assert_eq!(inputs.as_ffi()[1].type_, ffi::kLiteRtLmInputDataTypeImage);
        assert_eq!(inputs.as_ffi()[1].size, 3);
        assert_eq!(
            inputs.as_ffi()[2].type_,
            ffi::kLiteRtLmInputDataTypeImageEnd
        );
        assert!(inputs.as_ffi()[2].data.is_null());
        assert_eq!(inputs.as_ffi()[3].type_, ffi::kLiteRtLmInputDataTypeAudio);
        assert_eq!(
            inputs.as_ffi()[4].type_,
            ffi::kLiteRtLmInputDataTypeAudioEnd
        );
        assert_eq!(inputs._buffers.len(), 3);
    }
}
