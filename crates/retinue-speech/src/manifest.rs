use serde::{Deserialize, Serialize};

/// The sherpa-onnx recognizer family a model uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeechModelType {
    Transducer,
    Paraformer,
    Whisper,
    NemoCtc,
    SenseVoice,
    /// Cactus Needle-engine models (Whistle), run through the pinned `needle.exe` (see `needle.rs`).
    Needle,
}

/// Where inference happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeechProvider {
    Local,
    OpenAi,
}

/// One pinned file inside a model bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechDownloadFile {
    pub name: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
}

/// Everything the runtime needs to load, download, and describe a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelManifest {
    pub id: String,
    pub label: String,
    pub description: String,
    pub model_type: SpeechModelType,
    pub provider: SpeechProvider,
    pub language: String,
    pub files: Vec<String>,
    pub download_files: Vec<SpeechDownloadFile>,
    pub size_bytes: u64,
    pub sample_rate: u32,
    pub streaming: bool,
    #[serde(default)]
    pub modeling_unit: Option<String>,
    #[serde(default)]
    pub recommended: bool,
    /// Whether this build can download and run the model on the host it was
    /// compiled for. True by default, so an older client that ignores it still
    /// lists every model; Whistle is false off Windows (the Cactus Needle engine
    /// is published for Windows only).
    #[serde(default = "available_by_default")]
    pub available: bool,
}

fn available_by_default() -> bool {
    true
}

impl SpeechModelManifest {
    pub fn is_local(&self) -> bool {
        matches!(self.provider, SpeechProvider::Local)
    }

    /// Whether the engine this model needs runs on the platform this build was
    /// made for. `available` is the serialized form of this answer.
    pub fn available_on_this_platform(&self) -> bool {
        self.model_type != SpeechModelType::Needle || cfg!(windows)
    }
}
