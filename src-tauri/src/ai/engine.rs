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
pub struct InsightItem {
    pub kind: String, // "decision" | "action" | "risk" | "discussion"
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
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

        // Discard Whisper hallucination loops: any 4-gram repeating >3 times signals looping.
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() >= 16 {
            let looping = words.windows(4).enumerate().any(|(i, gram)| {
                words.windows(4).skip(i + 1).filter(|w| *w == gram).count() > 3
            });
            if looping {
                eprintln!("[Whisper] Hallucination loop detected, discarding chunk");
                return Ok(Vec::new());
            }
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

        let system_prompt = "You are Syntari, an on-device meeting note assistant.\n\
            Expand the user's shorthand into one precise sentence using ONLY the provided transcript.\n\
            STRICT RULES:\n\
            1. Every word in your output must be directly supported by a quote from the transcript.\n\
            2. Never add numbers, percentages, names, or dates not explicitly stated in the transcript.\n\
            3. If you cannot fully ground the expansion, return the raw shorthand unchanged — no paraphrasing.\n\
            4. One sentence maximum. No preamble, no 'Here is', no 'As discussed'.";

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

    /// Extract structured meeting items (decisions, actions, risks) from a transcript window.
    pub async fn extract_insights(
        &self,
        chunks: &[TranscriptChunk],
    ) -> Result<Vec<InsightItem>, String> {
        let session = match self.chat_session.as_ref() {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let context_text = chunks
            .iter()
            .map(|c| format!("{}: {}", c.speaker, c.text))
            .collect::<Vec<_>>()
            .join("\n");

        let system_prompt = "Extract key items from this meeting transcript segment.\n\
            Output ONLY lines in these exact formats (no other text):\n\
            DECISION: <what was decided>\n\
            ACTION: <owner> - <task> by <due date>\n\
            RISK: <risk or blocker>\n\
            DISCUSSION: <key topic discussed>\n\
            Rules: only output items explicitly stated. If nothing significant, output NONE.";

        let user_prompt = format!("TRANSCRIPT:\n{}\n\nEXTRACT ITEMS:", context_text);

        let request = Request::from_items(vec![
            Item::system_message(vec![Item::text(system_prompt)]),
            Item::user_message(vec![Item::text(user_prompt)]),
        ]);

        let response = session
            .process_request(request)
            .await
            .map_err(|e| format!("Inference error: {e}"))?;

        let mut raw = String::new();
        for item in &response.items {
            if let Some(t) = item.as_text() {
                raw.push_str(t);
            } else if let Some(msg) = item.as_message() {
                for part in &msg.content {
                    if let Some(t) = part.as_text() {
                        raw.push_str(t);
                    }
                }
            }
        }

        let raw = raw.trim().to_string();
        eprintln!("[Syntari Insights] raw: {:?}", &raw);

        // Discard hallucination loops (same check as Whisper)
        let words: Vec<&str> = raw.split_whitespace().collect();
        if words.len() >= 16 {
            let looping = words.windows(4).enumerate().any(|(i, gram)| {
                words.windows(4).skip(i + 1).filter(|w| *w == gram).count() > 3
            });
            if looping {
                eprintln!("[Syntari Insights] Hallucination loop detected, discarding");
                return Ok(Vec::new());
            }
        }

        let mut items = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if let Some(text) = line.strip_prefix("DECISION:") {
                items.push(InsightItem { kind: "decision".into(), text: text.trim().into(), owner: None, due: None });
            } else if let Some(rest) = line.strip_prefix("ACTION:") {
                let (owner, text, due) = parse_action_line(rest.trim());
                items.push(InsightItem { kind: "action".into(), text, owner: Some(owner), due });
            } else if let Some(text) = line.strip_prefix("RISK:") {
                items.push(InsightItem { kind: "risk".into(), text: text.trim().into(), owner: None, due: None });
            } else if let Some(text) = line.strip_prefix("DISCUSSION:") {
                items.push(InsightItem { kind: "discussion".into(), text: text.trim().into(), owner: None, due: None });
            }
        }
        Ok(items)
    }
}

fn parse_action_line(s: &str) -> (String, String, Option<String>) {
    let (owner, task_part) = s
        .split_once(" - ")
        .map(|(o, t)| (o.trim().to_string(), t.trim()))
        .unwrap_or_else(|| ("".into(), s));
    if let Some(idx) = task_part.rfind(" by ") {
        let task = task_part[..idx].trim().to_string();
        let due = task_part[idx + 4..].trim().to_string();
        (owner, task, Some(due))
    } else {
        (owner, task_part.to_string(), None)
    }
}
