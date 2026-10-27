use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Progress = Arc<dyn Fn(&str, f64) + Send + Sync>;
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
    pub category: String, // "decision" | "action" | "risk" | "discussion"
    pub owner: Option<String>,
    pub due: Option<String>,
    pub enhanced_text: String,
    pub raw_shorthand: String,
    pub citation_start: f64,
    pub citation_end: f64,
    pub citation_quote: String,
}

#[allow(dead_code)] // models are held only to keep them loaded for the sessions
pub struct FoundryEngine {
    speech_model: Option<Arc<Model>>,
    chat_model: Option<Arc<Model>>,
    audio_session: Option<AudioSession>,
    chat_session: Option<ChatSession>,
    is_ready: bool,
}

/// The only network access in the app: one-time model download when the cache is empty.
/// Manual update: delete the model folder (or drop files in `SYNTARI_MODELS_DIR`) and restart.
async fn ensure_cached(model: &Model, name: &str, progress: &Progress) -> Result<(), String> {
    if !model.is_cached().await.unwrap_or(false) {
        download(model, name, progress).await?;
    }
    verify_integrity(model, name).await
}

/// Trust-on-first-use integrity check: hashes are recorded after the download and re-checked
/// on every launch, so corruption or tampering of the weights is caught.
/// shortcut: no upstream-pinned hashes (the SDK doesn't publish any); pin them here when available.
async fn verify_integrity(model: &Model, name: &str) -> Result<(), String> {
    let dir = model.path().await.map_err(|e| format!("{name}: {e}"))?;
    tokio::task::spawn_blocking(move || {
        use sha2::{Digest, Sha256};
        let manifest = dir.join(".syntari-sha256");
        let mut current = std::collections::BTreeMap::new();
        for e in walkdir::WalkDir::new(&dir).sort_by_file_name().into_iter().filter_map(Result::ok) {
            if !e.file_type().is_file() || e.path() == manifest { continue; }
            let mut h = Sha256::new();
            std::io::copy(&mut std::fs::File::open(e.path()).map_err(|e| e.to_string())?, &mut h).map_err(|e| e.to_string())?;
            let rel = e.path().strip_prefix(&dir).unwrap().to_string_lossy().into_owned();
            current.insert(rel, format!("{:x}", h.finalize()));
        }
        match std::fs::read_to_string(&manifest) {
            Ok(saved) if serde_json::from_str::<std::collections::BTreeMap<String, String>>(&saved).ok().as_ref() == Some(&current) => Ok(()),
            Ok(_) => Err("model files changed since download (corrupt or tampered); delete the model folder to re-download".to_string()),
            Err(_) => std::fs::write(&manifest, serde_json::to_string(&current).unwrap()).map_err(|e| e.to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{name}: {e}"))
}

async fn download(model: &Model, name: &str, progress: &Progress) -> Result<(), String> {
    eprintln!("[Syntari] First run: downloading {name} (one-time)");
    let mut last = -10.0;
    let label = name.to_string();
    let report = progress.clone();
    model
        .download(Some(move |p: f64| {
            report(&label, p);
            if p - last >= 10.0 { last = p; eprintln!("[Syntari] {label}: {p:.0}%"); }
        }))
        .await
        .map_err(|e| format!("Download of {name} failed (offline? see README to import manually): {e}"))
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

    /// Initializes Foundry Local. Models are fetched once on first run, then used offline.
    /// Builds a ready engine without holding any lock, so a long first-run download never blocks callers.
    pub async fn load(models_dir: PathBuf, progress: Progress) -> Result<Self, String> {
        let models_dir_str = models_dir
            .to_str()
            .ok_or_else(|| "models directory path is not valid UTF-8".to_string())?;

        // 1. Configure SDK pointing strictly to local model cache and native dylib
        let mut config = FoundryLocalConfig::new("syntari-desktop")
            .log_level(LogLevel::Info)
            .model_cache_dir(models_dir_str);

        // Auto-detect native library location if not provided in environment
        if let Ok(lib_dir) = std::env::var("FOUNDRY_LOCAL_LIB_DIR") {
            let lib_path = PathBuf::from(lib_dir).join(format!("{}foundry_local{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX));
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
            .map_err(|e| format!("Foundry runtime failed to load (missing or damaged install; reinstall Syntari): {e}"))?;

        let catalog = manager.catalog();

        // 2. Load Speech-to-Text Model (whisper-small)
        let speech_model = catalog
            .get_model("whisper-small")
            .await
            .map_err(|e| format!("Whisper model lookup failed: {e}"))?;

        // Fail-safe check: Model must be pre-packaged in resource directory
        ensure_cached(&speech_model, "whisper-small", &progress).await?;

        speech_model
            .load()
            .await
            .map_err(|e| format!("Failed to load Whisper into memory: {e}"))?;

        let audio_session = AudioSession::new(&speech_model)
            .await
            .map_err(|e| format!("Failed to create AudioSession: {e}"))?;

        // 3. Load Chat / Expansion Model (phi-4-mini)
        let chat_model = catalog
            .get_model("phi-4-mini")
            .await
            .map_err(|e| format!("Chat model lookup failed: {e}"))?;

        ensure_cached(&chat_model, "phi-4-mini", &progress).await?;

        chat_model
            .load()
            .await
            .map_err(|e| format!("Failed to load Chat Model into memory: {e}"))?;

        let chat_session = ChatSession::new(&chat_model)
            .await
            .map_err(|e| format!("Failed to create ChatSession: {e}"))?;

        Ok(Self {
            speech_model: Some(speech_model),
            chat_model: Some(chat_model),
            audio_session: Some(audio_session),
            chat_session: Some(chat_session),
            is_ready: true,
        })
    }

    /// Transcribes an in-memory/RAM audio byte buffer using the local speech-to-text model session
    pub async fn transcribe_buffer(
        &self,
        wav_bytes: &[u8],
        speaker: &str,
    ) -> Result<Vec<TranscriptChunk>, String> {
        let session = self.audio_session.as_ref().ok_or("Audio session not initialized")?;

        eprintln!("[Whisper] Submitting {} bytes to whisper-small", wav_bytes.len());

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
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            opts.open(&temp_path)
                .and_then(|mut f| f.write_all(wav_bytes))
                .map_err(|e| format!("Failed to stage ephemeral audio: {e}"))?;
        }
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

        // Whisper emits "(soft music)" / "[BLANK_AUDIO]" for non-speech; not a transcript.
        let bare = trimmed.trim_matches(|c: char| c.is_whitespace() || "()[]*♪.".contains(c));
        let wrapped = (trimmed.starts_with('(') && trimmed.ends_with(')')) || (trimmed.starts_with('[') && trimmed.ends_with(']'));
        if bare.is_empty() || wrapped {
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
                    category: "discussion".into(),
                    owner: None,
                    due: None,
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

        // 2. Format context snippet
        let mut context_text = String::new();
        for c in &context_chunks {
            context_text.push_str(&format!("[{:.1}s - {:.1}s] {}: {}\n", c.start, c.end, c.speaker, c.text));
        }

        if context_text.is_empty() {
            context_text.push_str("(no transcript context)\n");
        }

        let system_prompt = "You are Syntari, an on-device meeting note assistant.\n\
            First classify the note, then expand it into one precise sentence using ONLY the provided transcript.\n\
            Reply in exactly this format:\n\
            CATEGORY: decision | action | risk | discussion\n\
            Category definitions:\n\
            - decision: something already agreed or chosen (\"We agreed to ship Friday\", \"Going with Postgres\").\n\
            - action: a task someone must do, usually with a person or deadline (\"Dana will send the contract\").\n\
            - risk: a problem, delay, cost, or blocker that could hurt the plan.\n\
            - discussion: a topic talked about, an idea, or a tentative suggestion (\"Maybe we should switch vendors\") with no commitment.\n\
            When unsure, choose discussion.\n\
            NOTE: <one sentence>\n\
            OWNER: <person responsible, or none>\n\
            DUE: <deadline as stated, or none>\n\
            OWNER and DUE only apply to actions; write none otherwise.\n\
            STRICT RULES:\n\
            1. Every word in your output must be directly supported by a quote from the transcript.\n\
            2. Never add numbers, percentages, names, or dates not explicitly stated in the transcript.\n\
            3. If you cannot fully ground the expansion, return the raw shorthand unchanged — no paraphrasing.\n\
            4. One sentence maximum. No preamble, no 'Here is', no 'As discussed'.";

        let user_prompt = format!(
            "CONVERSATION CONTEXT:\n{}\n\nUSER SHORTHAND NOTE:\n\"{}\"\n\nCATEGORY:",
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

        // The model may or may not repeat "CATEGORY:", so take the first category word on any line before NOTE.
        let mut category = "discussion".to_string();
        let mut note = String::new();
        let (mut owner, mut due) = (None, None);
        let mut found = false;
        for line in enhanced_text.lines() {
            let l = line.trim();
            let field = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("none"));
            if let Some(n) = l.strip_prefix("NOTE:") {
                note = n.trim().to_string();
            } else if let Some(v) = l.strip_prefix("OWNER:") {
                owner = field(v);
            } else if let Some(v) = l.strip_prefix("DUE:") {
                due = field(v);
            } else if !found {
                let lower = l.to_lowercase();
                if let Some(c) = ["decision", "action", "risk", "discussion"].iter().find(|c| lower.contains(*c)) {
                    category = c.to_string();
                    found = true;
                }
            }
        }
        if note.is_empty() {
            note = raw_shorthand.to_string();
        }
        eprintln!("[Syntari Inference] raw=\"{raw_shorthand}\" -> {category}: \"{note}\"");

        let (citation_start, citation_end) = match (context_chunks.first(), context_chunks.last()) {
            (Some(f), Some(l)) => (f.start, l.end),
            _ => (keystroke_time, keystroke_time),
        };

        Ok(EnhancedResult {
            category,
            owner,
            due,
            enhanced_text: note,
            raw_shorthand: raw_shorthand.to_string(),
            citation_start,
            citation_end,
            citation_quote: context_chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "),
        })
    }

    /// One-shot LLM call; returns the concatenated text of the response.
    async fn complete(&self, system: &str, user: String) -> Result<String, String> {
        let session = self.chat_session.as_ref().ok_or("AI model not loaded")?;
        let request = Request::from_items(vec![
            Item::system_message(vec![Item::text(system)]),
            Item::user_message(vec![Item::text(user)]),
        ]);
        let response = session.process_request(request).await.map_err(|e| format!("Inference error: {e}"))?;
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
        Ok(raw.trim().to_string())
    }

    /// Summarize a whole transcript. Long transcripts are summarized in segments, then merged.
    pub async fn summarize(&self, chunks: &[TranscriptChunk]) -> Result<String, String> {
        if chunks.is_empty() {
            return Err("Transcript is empty".into());
        }
        const SEGMENT_CHARS: usize = 6000; // keeps each prompt well inside the small model's context
        let sys = "You summarize meeting transcripts. Write a concise markdown summary: a short overview paragraph, \
            then bullet lists under '### Key Points', '### Decisions', '### Action Items' (omit empty sections). \
            Use only what the transcript states.";
        let mut segments = vec![String::new()];
        for c in chunks {
            let line = format!("{}: {}\n", c.speaker, c.text);
            if segments.last().unwrap().len() + line.len() > SEGMENT_CHARS && !segments.last().unwrap().is_empty() {
                segments.push(String::new());
            }
            segments.last_mut().unwrap().push_str(&line);
        }
        let mut partials = Vec::new();
        for seg in segments {
            partials.push(self.complete(sys, format!("TRANSCRIPT:\n{seg}\nSUMMARY:")).await?);
        }
        if partials.len() == 1 {
            return Ok(partials.remove(0));
        }
        self.complete(sys, format!("Merge these partial summaries of one meeting into a single summary:\n\n{}\n\nSUMMARY:", partials.join("\n\n---\n\n"))).await
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
        for line in raw.lines().filter(|l| {
            let t = l.rsplit(':').next().unwrap_or("").trim().trim_end_matches('.').to_lowercase();
            !matches!(t.as_str(), "" | "none" | "none provided" | "n/a" | "none stated")
        }) {
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
