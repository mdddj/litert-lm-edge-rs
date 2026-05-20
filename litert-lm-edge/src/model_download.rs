use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const DEFAULT_HUGGING_FACE_BASE_URL: &str = "https://huggingface.co";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownModel {
    Gemma4E2B,
    Gemma4E4B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: KnownModel,
    pub name: &'static str,
    pub repo: &'static str,
    pub filename: &'static str,
    pub revision: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

pub struct ModelDownloader {
    base_url: String,
    user_agent: String,
}

impl KnownModel {
    pub fn info(self) -> ModelInfo {
        match self {
            Self::Gemma4E2B => ModelInfo {
                id: self,
                name: "Gemma 4 E2B",
                repo: "litert-community/gemma-4-E2B-it-litert-lm",
                filename: "gemma-4-E2B-it.litertlm",
                revision: "73d35ec36cf24347ab4eec1a46f0aafbb9c3a89d",
                sha256: "181938105e0eefd105961417e8da75903eacda102c4fce9ce90f50b97139a63c",
                size_bytes: 2_588_147_712,
                description: "Next-gen multimodal chat — text, image, audio.",
            },
            Self::Gemma4E4B => ModelInfo {
                id: self,
                name: "Gemma 4 E4B",
                repo: "litert-community/gemma-4-E4B-it-litert-lm",
                filename: "gemma-4-E4B-it.litertlm",
                revision: "4f479a5ff97de64f5c1711ec439a2cb89e6a8fb4",
                sha256: "0b2a8980ce155fd97673d8e820b4d29d9c7d99b8fa6806f425d969b145bd52e0",
                size_bytes: 3_659_530_240,
                description: "Next-gen multimodal chat — text, image, audio.",
            },
        }
    }

    pub fn all() -> &'static [KnownModel] {
        &[Self::Gemma4E2B, Self::Gemma4E4B]
    }
}

impl ModelInfo {
    pub fn download_url(&self, base_url: &str) -> String {
        let base_url = base_url.trim_end_matches('/');
        format!(
            "{}/{}/resolve/{}/{}",
            base_url, self.repo, self.revision, self.filename
        )
    }

    pub fn default_dir_name(&self) -> &'static str {
        match self.id {
            KnownModel::Gemma4E2B => "gemma-4-E2B-it-litert-lm",
            KnownModel::Gemma4E4B => "gemma-4-E4B-it-litert-lm",
        }
    }
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_HUGGING_FACE_BASE_URL.to_owned(),
            user_agent: format!("litert-lm-edge/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl ModelDownloader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    pub fn download_to_dir(
        &self,
        model: KnownModel,
        root_dir: impl AsRef<Path>,
    ) -> Result<PathBuf> {
        self.download_to_dir_with_progress(model, root_dir, |_| {})
    }

    pub fn download_to_dir_with_progress<F>(
        &self,
        model: KnownModel,
        root_dir: impl AsRef<Path>,
        on_progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(DownloadProgress),
    {
        let info = model.info();
        let model_dir = root_dir.as_ref().join(info.default_dir_name());
        self.download_to_path_with_progress(model, model_dir.join(info.filename), on_progress)
    }

    pub fn download_to_path(
        &self,
        model: KnownModel,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf> {
        self.download_to_path_with_progress(model, destination, |_| {})
    }

    pub fn download_to_path_with_progress<F>(
        &self,
        model: KnownModel,
        destination: impl AsRef<Path>,
        mut on_progress: F,
    ) -> Result<PathBuf>
    where
        F: FnMut(DownloadProgress),
    {
        let info = model.info();
        let destination = destination.as_ref();
        if destination.is_file() {
            verify_sha256(destination, info.sha256)?;
            return Ok(destination.to_path_buf());
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        let partial = partial_path(destination);
        let url = info.download_url(&self.base_url);
        let response = ureq::get(&url)
            .set("User-Agent", &self.user_agent)
            .call()
            .map_err(Box::new)?;
        let total_bytes = response
            .header("content-length")
            .and_then(|value| value.parse().ok())
            .or(Some(info.size_bytes));

        let mut reader = response.into_reader();
        let mut writer = BufWriter::new(File::create(&partial)?);
        let mut downloaded_bytes = 0_u64;
        let mut buffer = [0_u8; 1024 * 1024];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
            downloaded_bytes += read as u64;
            on_progress(DownloadProgress {
                downloaded_bytes,
                total_bytes,
            });
        }
        writer.flush()?;
        drop(writer);

        verify_sha256(&partial, info.sha256)?;
        fs::rename(&partial, destination)?;
        Ok(destination.to_path_buf())
    }
}

pub fn verify_sha256(path: impl AsRef<Path>, expected: &str) -> Result<()> {
    let path = path.as_ref();
    let actual = sha256_file(path)?;
    if actual != expected {
        return Err(Error::ChecksumMismatch {
            path: path.display().to_string(),
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}

pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn partial_path(destination: &Path) -> PathBuf {
    let mut filename = destination
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "download".into());
    filename.push(".partial");
    destination.with_file_name(filename)
}

impl From<ureq::Error> for Error {
    fn from(error: ureq::Error) -> Self {
        Self::Http(Box::new(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models_have_stable_metadata() {
        let models = KnownModel::all();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].info().filename, "gemma-4-E2B-it.litertlm");
        assert_eq!(models[1].info().filename, "gemma-4-E4B-it.litertlm");
    }

    #[test]
    fn builds_hugging_face_download_url() {
        let info = KnownModel::Gemma4E4B.info();
        assert_eq!(
            info.download_url("https://huggingface.co/"),
            "https://huggingface.co/litert-community/gemma-4-E4B-it-litert-lm/resolve/4f479a5ff97de64f5c1711ec439a2cb89e6a8fb4/gemma-4-E4B-it.litertlm"
        );
    }

    #[test]
    fn creates_partial_path() {
        assert_eq!(
            partial_path(Path::new("/tmp/model.litertlm")),
            PathBuf::from("/tmp/model.litertlm.partial")
        );
    }
}
