use std::path::{Path, PathBuf};
use std::sync::Arc;
use foundry_local_sdk::{
    AudioSession, ChatSession, FoundryLocalConfig, FoundryLocalManager, Item, LogLevel, Model, Request,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TranscriptChunk {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EnhancedResult {
    pub enhanced_text: String,
    pub raw_shorthand: String,
    pub citation_start: f64,
    pub citation_end: f64,
    pub citation_quote: String,
}

pub struct FoundryEngine {
    speech_model: Option<Arc<Model>>,
    chat_model: Option<Arc<Model>>,
    audio_session: Option<AudioSession>,
    chat_session: Option<ChatSession>,
    is_ready: bool,
}

impl FoundryEngine {
    pub fn new() -> Self {
        Self {
            speech_model: None,
            chat_model: None,
            audio_session: None,
            chat_session: None,
            is_ready: false,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.is_ready
    }

    /// Initializes Foundry Local using local cached models (Air-Gapped, No Downloads)
    pub async fn initialize(&mut self, models_dir: PathBuf) -> Result<(), String> {
        let models_dir_str = models_dir
            .to_str()
            .ok_or_else(|| "models directory path is not valid UTF-8".to_string())?;

        // 1. Configure SDK pointing strictly to local model cache and native dylib
        let mut config = FoundryLocalConfig::new("syntari-desktop")
            .log_level(LogLevel::Info)
            .model_cache_dir(models_dir_str);

        // Auto-detect native library location if not provided in environment
        if let Ok(lib_dir) = std::env::var("FOUNDRY_LOCAL_LIB_DIR") {
            let lib_path = PathBuf::from(lib_dir).join("libfoundry_local.dylib");
            if lib_path.exists() {
                config = config.library_path(lib_path.to_string_lossy());
            }
        } else {
            let candidates = [
                "/opt/homebrew/Cellar/foundrylocal/0.11.0_1/libexec/libfoundry_local.dylib",
                "/opt/homebrew/lib/libfoundry_local.dylib",
                "/usr/local/lib/libfoundry_local.dylib",
            ];
            for candidate in candidates {
                if Path::new(candidate).exists() {
                    config = config.library_path(candidate);
                    break;
                }
            }
        }

        let manager = FoundryLocalManager::create(config)
            .map_err(|e| format!("Failed to create FoundryLocalManager: {e}"))?;

        let catalog = manager.catalog();

        // 2. Load Speech-to-Text Model (whisper-tiny)
        let speech_model = catalog
            .get_model("whisper-tiny")
            .await
            .map_err(|e| format!("Whisper model lookup failed: {e}"))?;

        // Fail-safe check: Model must be pre-packaged in resource directory
        if !speech_model.is_cached().await.unwrap_or(false) {
            return Err("Whisper model is not pre-packaged in local resources.".into());
        }

        speech_model
            .load()
            .await
            .map_err(|e| format!("Failed to load Whisper into memory: {e}"))?;

        let audio_session = AudioSession::new(&speech_model)
            .await
            .map_err(|e| format!("Failed to create AudioSession: {e}"))?;

        // 3. Load Chat / Expansion Model (qwen2.5-0.5b or phi-4-mini)
        let chat_model = catalog
            .get_model("qwen2.5-0.5b")
            .await
            .map_err(|e| format!("Chat model lookup failed: {e}"))?;

        if !chat_model.is_cached().await.unwrap_or(false) {
            return Err("Expansion chat model is not pre-packaged in local resources.".into());
        }

        chat_model
            .load()
            .await
            .map_err(|e| format!("Failed to load Chat Model into memory: {e}"))?;

        let chat_session = ChatSession::new(&chat_model)
            .await
            .map_err(|e| format!("Failed to create ChatSession: {e}"))?;

        self.speech_model = Some(speech_model);
        self.chat_model = Some(chat_model);
        self.audio_session = Some(audio_session);
        self.chat_session = Some(chat_session);
        self.is_ready = true;

        Ok(())
    }

    /// Transcribes an in-memory/RAM audio byte buffer using the local speech-to-text model session
    pub async fn transcribe_buffer(
        &self,
        wav_bytes: &[u8],
        speaker: &str,
    ) -> Result<Vec<TranscriptChunk>, String> {
        let session = self.audio_session.as_ref().ok_or("Audio session not initialized")?;

        eprintln!("[Whisper] Submitting {} bytes to whisper-tiny", wav_bytes.len());

        // Zero-forensic RAII ephemeral file handler:
        // Foundry Local SDK requires a file URI on macOS/Darwin runtime.
        struct EphemeralAudioFile(PathBuf, usize);
        impl Drop for EphemeralAudioFile {
            fn drop(&mut self) {
                if self.0.exists() {
                    // Forensically overwrite with zero bytes before unlinking
                    let zeros = vec![0u8; self.1];
                    let _ = std::fs::write(&self.0, zeros);
                    let _ = std::fs::remove_file(&self.0);
                }
            }
        }

        let temp_path = std::env::temp_dir().join(format!("syntari_ephemeral_{}_{}.wav", fastrand::u64(..), fastrand::u32(..)));
        std::fs::write(&temp_path, wav_bytes)
            .map_err(|e| format!("Failed to stage ephemeral audio: {e}"))?;
        let _guard = EphemeralAudioFile(temp_path.clone(), wav_bytes.len());

        let path_str = temp_path.to_string_lossy().to_string();
        let item = Item::audio_uri(&path_str, Some("audio/wav"), 16_000, 1);
        let text = session
            .transcribe(item)
            .await
            .map_err(|e| format!("Transcription error: {e}"))?;

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![TranscriptChunk {
            start: 0.0,
            end: 0.0,
            speaker: speaker.to_string(),
            text: trimmed.to_string(),
        }])
    }

    /// Contextual Expansion using the [T - 45s, T + 10s] window
    pub async fn expand_shorthand(
        &self,
        raw_shorthand: &str,
        keystroke_time: f64,
        transcript_history: &[TranscriptChunk],
    ) -> Result<EnhancedResult, String> {
        let session = match self.chat_session.as_ref() {
            Some(s) => s,
            None => {
                // Graceful fallback when model is not yet loaded: preserve raw shorthand
                return Ok(EnhancedResult {
                    enhanced_text: raw_shorthand.to_string(),
                    raw_shorthand: raw_shorthand.to_string(),
                    citation_start: keystroke_time,
                    citation_end: keystroke_time,
                    citation_quote: "Offline AI model not yet initialized; preserved original shorthand.".into(),
                });
            }
        };

        // 1. Slice the temporal context window [-45s, +10s]
        let t_min = (keystroke_time - 45.0).max(0.0);
        let t_max = keystroke_time + 10.0;

        let context_chunks: Vec<&TranscriptChunk> = transcript_history
            .iter()
            .filter(|c| c.end >= t_min && c.start <= t_max)
            .collect();

        // If no relevant conversation occurred in this window, preserve raw shorthand
        if context_chunks.is_empty() {
            return Ok(EnhancedResult {
                enhanced_text: raw_shorthand.to_string(),
                raw_shorthand: raw_shorthand.to_string(),
                citation_start: keystroke_time,
                citation_end: keystroke_time,
                citation_quote: "No temporal transcript context found; preserved original.".into(),
            });
        }

        // 2. Format context snippet
        let mut context_text = String::new();
        for c in &context_chunks {
            context_text.push_str(&format!("[{:.1}s - {:.1}s] {}: {}\n", c.start, c.end, c.speaker, c.text));
        }

        let system_prompt = "You are Syntari, an on-device executive meeting synthesizer.\n\
            Your task is to expand the user's shorthand notes into precise, professional, and factual statements.\n\
            RULES:\n\
            1. Rely STRICTLY on the facts stated in the provided transcript context.\n\
            2. Never hallucinate or infer numbers, dates, or decisions not explicitly stated.\n\
            3. If the shorthand is a personal reminder not discussed, return the shorthand unchanged.\n\
            4. Keep output concise (1-2 sentences max). Do not preface with 'Here is...' or conversational filler.";

        let user_prompt = format!(
            "CONVERSATION CONTEXT:\n{}\n\nUSER SHORTHAND NOTE:\n\"{}\"\n\nEXPANDED FACTUAL NOTE:",
            context_text, raw_shorthand
        );

        // 3. Process structured conversation request through session
        let request = Request::from_items(vec![
            Item::system_message(vec![Item::text(system_prompt)]),
            Item::user_message(vec![Item::text(user_prompt)]),
        ]);

        let response = session
            .process_request(request)
            .await
            .map_err(|e| format!("Inference error: {e}"))?;

        let mut enhanced_text = String::new();
        for item in &response.items {
            if let Some(t) = item.as_text() {
                enhanced_text.push_str(t);
            } else if let Some(msg) = item.as_message() {
                for part in &msg.content {
                    if let Some(t) = part.as_text() {
                        enhanced_text.push_str(t);
                    }
                }
            }
        }

        eprintln!(
            "[Syntari Inference] raw=\"{}\" -> enhanced=\"{}\"",
            raw_shorthand,
            enhanced_text.trim()
        );

        if enhanced_text.trim().is_empty() {
            enhanced_text = raw_shorthand.to_string();
        }

        let first_chunk = context_chunks.first().unwrap();
        let last_chunk = context_chunks.last().unwrap();

        Ok(EnhancedResult {
            enhanced_text: enhanced_text.trim().to_string(),
            raw_shorthand: raw_shorthand.to_string(),
            citation_start: first_chunk.start,
            citation_end: last_chunk.end,
            citation_quote: context_chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "),
        })
    }
}
