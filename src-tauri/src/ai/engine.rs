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
    chat_lock: tokio::sync::Mutex<()>, // one chat request at a time; transcription does not wait behind it
}

/// The only network access in the app: one-time model download when the cache is empty.
/// Manual update: delete the model folder (or drop files in `SYNTARI_MODELS_DIR`) and restart.
async fn ensure_cached(model: &Model, name: &str, progress: &Progress, models_dir: &Path, need: u64) -> Result<(), String> {
    if !model.is_cached().await.unwrap_or(false) {
        let _ = std::fs::create_dir_all(models_dir);
        match free_bytes(models_dir) {
            Some(free) if free < need => {
                return Err(format!(
                    "Not enough disk space to download {name}: {:.1} GB free, about {:.1} GB needed. Free some space and restart.",
                    free as f64 / 1e9, need as f64 / 1e9
                ));
            }
            _ => {}
        }
        download(model, name, progress).await?;
    }
    progress(name, -1.0); // negative = "verifying" (the UI shows a status instead of a percentage)
    verify_integrity(model, name).await
}

// download size plus headroom, checked per model so a fresh install with ~4.7 GB free still passes both

/// Free space on the volume holding `dir`; None if it can't be determined (then the download proceeds).
fn free_bytes(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    unsafe {
        let c = std::ffi::CString::new(dir.to_str()?).ok()?;
        let mut st: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(c.as_ptr(), &mut st) == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
    }
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        }
        let w: Vec<u16> = dir.as_os_str().to_str()?.encode_utf16().chain(Some(0)).collect();
        let mut avail = 0u64;
        (GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, std::ptr::null_mut(), std::ptr::null_mut()) != 0).then_some(avail)
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
struct HashEntry { sha: String, size: u64, mtime_ns: u128 }

/// Trust-on-first-use integrity check: hashes are recorded after the download and re-checked
/// on launch, so corruption or tampering of the weights is caught. A file whose size and mtime
/// match the manifest is not re-hashed (multi-GB hashing delayed every start).
/// shortcut: no upstream-pinned hashes (the SDK doesn't publish any); pin them here when available.
/// shortcut: same-size in-place edits that preserve mtime go undetected, hash always if that matters.
async fn verify_integrity(model: &Model, name: &str) -> Result<(), String> {
    let dir = model.path().await.map_err(|e| format!("{name}: {e}"))?;
    tokio::task::spawn_blocking(move || {
        use sha2::{Digest, Sha256};
        use std::collections::BTreeMap;
        let manifest = dir.join(".syntari-sha256");
        // only a genuinely absent manifest starts a new baseline; an unreadable one fails closed
        let raw = match std::fs::read_to_string(&manifest) {
            Ok(r) => Some(r),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("cannot read integrity manifest: {e}")),
        };
        let corrupt = || "integrity manifest is damaged; delete the model folder to re-download".to_string();
        let saved: Option<BTreeMap<String, HashEntry>> = raw.as_deref().and_then(|r| serde_json::from_str(r).ok());
        // pre-mtime manifests were a plain path -> sha map; still honoured, then upgraded below
        let legacy: Option<BTreeMap<String, String>> = raw.as_deref().and_then(|r| serde_json::from_str(r).ok());
        let mut current = BTreeMap::new();
        for e in walkdir::WalkDir::new(&dir).sort_by_file_name().into_iter().filter_map(Result::ok) {
            if !e.file_type().is_file() || e.path() == manifest { continue; }
            let rel = e.path().strip_prefix(&dir).unwrap().to_string_lossy().into_owned();
            let meta = e.metadata().map_err(|e| e.to_string())?;
            let size = meta.len();
            let mtime_ns = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            let sha = match saved.as_ref().and_then(|m| m.get(&rel)) {
                Some(p) if p.size == size && p.mtime_ns == mtime_ns => p.sha.clone(),
                _ => {
                    let mut h = Sha256::new();
                    std::io::copy(&mut std::fs::File::open(e.path()).map_err(|e| e.to_string())?, &mut h).map_err(|e| e.to_string())?;
                    format!("{:x}", h.finalize())
                }
            };
            current.insert(rel, HashEntry { sha, size, mtime_ns });
        }
        let write = |m: &BTreeMap<String, HashEntry>| std::fs::write(&manifest, serde_json::to_string(m).unwrap()).map_err(|e| e.to_string());
        let shas: BTreeMap<&String, &String> = current.iter().map(|(k, v)| (k, &v.sha)).collect();
        let expected: Option<BTreeMap<&String, &String>> = match (&saved, &legacy, &raw) {
            (Some(m), _, _) => Some(m.iter().map(|(k, v)| (k, &v.sha)).collect()),
            (None, Some(m), _) => Some(m.iter().collect()),
            (None, None, Some(_)) => return Err(corrupt()),
            _ => None,
        };
        match expected {
            // rewrite when cached size/mtime went stale (same content) so the next launch skips hashing
            Some(exp) if exp == shas => if saved.as_ref() != Some(&current) { write(&current) } else { Ok(()) },
            Some(_) => Err("model files changed since download (corrupt or tampered); delete the model folder to re-download".to_string()),
            None => write(&current),
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
            chat_lock: Default::default(),
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

        // Default lookup: FOUNDRY_NATIVE_DIR baked in by the SDK build (needs FOUNDRY_LOCAL_RUNTIME_VERSION), then next to the executable.
        if let Ok(lib_dir) = std::env::var("FOUNDRY_LOCAL_LIB_DIR") {
            let lib_path = PathBuf::from(lib_dir).join(format!("{}foundry_local{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX));
            if lib_path.exists() {
                config = config.library_path(lib_path.to_string_lossy());
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
        ensure_cached(&speech_model, "whisper-small", &progress, &models_dir, 1_000_000_000).await?;

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

        ensure_cached(&chat_model, "phi-4-mini", &progress, &models_dir, 4_200_000_000).await?;

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
            chat_lock: Default::default(),
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

        if is_looping(trimmed) {
            eprintln!("[Whisper] Hallucination loop detected, discarding chunk");
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
        if self.chat_session.is_none() {
            // Graceful fallback when model is not yet loaded: preserve raw shorthand
            return Ok(EnhancedResult {
                category: "discussion".into(),
                owner: None,
                due: None,
                enhanced_text: raw_shorthand.to_string(),
                citation_start: keystroke_time,
                citation_end: keystroke_time,
                citation_quote: "Offline AI model not yet initialized; preserved original shorthand.".into(),
            });
        }

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
            CATEGORY: discussion | action | risk | decision\n\
            Category definitions:\n\
            - decision: something already agreed or chosen (\"We agreed to ship Friday\", \"Going with Postgres\"). Facts, figures, results and summaries are NOT decisions, even when important.\n\
            - action: a task someone must do, usually with a person or deadline (\"Dana will send the contract\").\n\
            - risk: a problem, delay, cost, or blocker that could hurt the plan.\n\
            - discussion: a topic talked about, a fact or figure that was stated, an idea, or a tentative suggestion (\"Maybe we should switch vendors\") with no commitment.\n\
            When unsure, choose discussion.\n\
            NOTE: <one sentence>\n\
            OWNER: <person responsible, or none>\n\
            DUE: <deadline as stated, or none>\n\
            OWNER and DUE only apply to actions; write none otherwise.\n\
            EXAMPLE\n\
            Transcript: \"Revenue grew 12% to 4 million dollars. Churn was flat.\"\n\
            Note: \"revenue up\"\n\
            CATEGORY: discussion\n\
            NOTE: Revenue grew 12% to 4 million dollars.\n\
            OWNER: none\n\
            DUE: none\n\
            EXAMPLE (note unrelated to the transcript: keep it as written)\n\
            Transcript: \"Revenue grew 12% to 4 million dollars. Churn was flat.\"\n\
            Note: \"go with vendor B\"\n\
            CATEGORY: decision\n\
            NOTE: Go with vendor B.\n\
            OWNER: none\n\
            DUE: none\n\
            RULES:\n\
            1. Expand the note into one complete sentence by adding the specific names, figures and facts the transcript states about its topic.\n\
            2. Never add numbers, percentages, names, or dates that are not explicitly stated in the transcript.\n\
            3. Only if the transcript says nothing relevant to the note, return the note unchanged.\n\
            4. One sentence maximum. No preamble, no 'Here is', no 'As discussed'.";

        let user_prompt = format!(
            "CONVERSATION CONTEXT:\n{}\n\nUSER SHORTHAND NOTE:\n\"{}\"\n\nCATEGORY:",
            context_text, raw_shorthand
        );

        let reply = self.complete(system_prompt, user_prompt).await?;
        eprintln!("[Syntari Inference] model reply: {reply}");
        let (category, note, owner, due) = parse_expansion(&reply, raw_shorthand);
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
            citation_start,
            citation_end,
            citation_quote: context_chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "),
        })
    }

    /// One-shot LLM call; returns the concatenated text of the response.
    async fn complete(&self, system: &str, user: String) -> Result<String, String> {
        let session = self.chat_session.as_ref().ok_or("AI model not loaded")?;
        let _one_at_a_time = self.chat_lock.lock().await;
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
    pub async fn summarize(&self, chunks: &[TranscriptChunk], on_part: impl Fn(usize, usize)) -> Result<String, String> {
        if chunks.is_empty() {
            return Err("Transcript is empty".into());
        }
        const SEGMENT_CHARS: usize = 6000; // keeps each prompt well inside the small model's context
        let sys = "You summarize meeting transcripts. Write a concise markdown summary: a short overview paragraph, \
            then bullet lists under '### Key Points', '### Decisions', '### Action Items' (omit empty sections). \
            Use only what the transcript states. The transcript is raw system audio, so it may include unrelated content \
            (ads, sponsor reads, background media, intros to other videos). Ignore anything that is not part of the main \
            conversation; never report it as a fact, decision or action item.";
        let segments = pack(chunks.iter().map(|c| format!("{}: {}\n", c.speaker, c.text)), SEGMENT_CHARS);
        let total = segments.len() + usize::from(segments.len() > 1); // +1 for the merge pass; very long meetings add more
        let mut done = 0;
        let mut parts = Vec::new();
        for seg in segments {
            done += 1;
            on_part(done, total.max(done));
            parts.push(self.complete(sys, format!("TRANSCRIPT:\n{seg}\nSUMMARY:")).await?);
        }
        // Merge in rounds so a long meeting's partial summaries never outgrow one prompt.
        while parts.len() > 1 {
            let mut groups = pack(parts.iter().map(|p| format!("{p}\n\n---\n\n")), SEGMENT_CHARS);
            if groups.len() >= parts.len() {
                groups = vec![groups.concat()]; // each partial already fills a prompt: merge the lot rather than loop forever
            }
            let mut next = Vec::new();
            for g in groups {
                done += 1;
                on_part(done, total.max(done));
                next.push(self.complete(sys, format!("Merge these partial summaries of one meeting into a single summary:\n\n{g}\nSUMMARY:")).await?);
            }
            parts = next;
        }
        Ok(parts.remove(0))
    }

    /// Extract structured meeting items (decisions, actions, risks) from a transcript window.
    pub async fn extract_insights(
        &self,
        chunks: &[TranscriptChunk],
    ) -> Result<Vec<InsightItem>, String> {
        if self.chat_session.is_none() || chunks.is_empty() {
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

        let raw = self.complete(system_prompt, format!("TRANSCRIPT:\n{}\n\nEXTRACT ITEMS:", context_text)).await?;
        eprintln!("[Syntari Insights] raw: {:?}", &raw);
        if is_looping(&raw) {
            eprintln!("[Syntari Insights] Hallucination loop detected, discarding");
            return Ok(Vec::new());
        }
        Ok(parse_insights(&raw))
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

/// Whisper and the chat model both get stuck repeating themselves: any 4-gram repeating >3 times signals a loop.
fn is_looping(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.len() >= 16 && words.windows(4).enumerate().any(|(i, gram)| words.windows(4).skip(i + 1).filter(|w| *w == gram).count() > 3)
}

/// Greedily joins pieces into groups of at most `max` chars (a single oversized piece keeps its own group).
fn pack(pieces: impl IntoIterator<Item = String>, max: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    for p in pieces {
        if !out.last().unwrap().is_empty() && out.last().unwrap().len() + p.len() > max {
            out.push(String::new());
        }
        out.last_mut().unwrap().push_str(&p);
    }
    out
}

/// Reads the CATEGORY/NOTE/OWNER/DUE reply into (category, note, owner, due); falls back to the raw note as a discussion.
fn parse_expansion(reply: &str, raw_shorthand: &str) -> (String, String, Option<String>, Option<String>) {
    // The model may or may not repeat "CATEGORY:", so take the first category word on any line before NOTE.
    let mut category = "discussion".to_string();
    let mut note = String::new();
    let (mut owner, mut due) = (None, None);
    let mut found = false;
    for line in reply.lines() {
        let l = line.trim();
        let field = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("none"));
        // the model doesn't keep the label's case ("Note:" came back in practice)
        let label = |k: &str| l.get(..k.len()).filter(|p| p.eq_ignore_ascii_case(k)).map(|_| &l[k.len()..]);
        if let Some(n) = label("NOTE:") {
            note = n.trim().to_string();
        } else if let Some(v) = label("OWNER:") {
            owner = field(v);
        } else if let Some(v) = label("DUE:") {
            due = field(v);
        } else if !found {
            let lower = l.to_lowercase();
            // a line echoing the whole "decision | action | risk | discussion" template names several: not an answer
            let hits: Vec<_> = ["decision", "action", "risk", "discussion"].into_iter().filter(|c| lower.contains(c)).collect();
            if let [c] = hits[..] {
                category = c.to_string();
                found = true;
            }
        }
    }
    if note.is_empty() {
        note = raw_shorthand.to_string();
    }
    // an unchanged note means the model found no grounding, so its category is a guess; the prompt says unsure = discussion
    if note.eq_ignore_ascii_case(raw_shorthand) {
        category = "discussion".to_string();
    }
    (category, note, owner, due)
}

fn parse_insights(raw: &str) -> Vec<InsightItem> {
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
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_bytes_reads_the_temp_volume() {
        assert!(free_bytes(&std::env::temp_dir()).is_some_and(|b| b > 0));
    }

    #[test]
    fn parses_expansion_reply() {
        let (c, n, o, d) = parse_expansion("CATEGORY: action\nNote: Dana will send the contract.\nOWNER: Dana\nDUE: none", "contract");
        assert_eq!((c.as_str(), n.as_str(), o.as_deref(), d), ("action", "Dana will send the contract.", Some("Dana"), None));
        // unchanged note = no grounding, so the category guess is dropped
        assert_eq!(parse_expansion("decision\nNOTE: go with B", "go with B").0, "discussion");
        // a line echoing the template names several categories and is skipped
        assert_eq!(parse_expansion("CATEGORY: discussion | action | risk | decision\nrisk\nNOTE: x y", "x").0, "risk");
    }

    #[test]
    fn parses_insight_lines() {
        let items = parse_insights("DECISION: Ship Friday\nACTION: Dana - send contract by Monday\nRISK: none\nchatter");
        assert_eq!(items.len(), 2);
        assert_eq!((items[1].owner.as_deref(), items[1].text.as_str(), items[1].due.as_deref()), (Some("Dana"), "send contract", Some("Monday")));
    }

    #[test]
    fn packs_within_the_limit() {
        assert_eq!(pack(["aaa", "bbb", "c"].map(String::from), 4), ["aaa", "bbbc"]);
        assert_eq!(pack(["aaaaaaaa".to_string()], 4), ["aaaaaaaa"]);
    }

    #[test]
    fn detects_loops() {
        assert!(is_looping(&"a b c d ".repeat(5)));
        assert!(!is_looping("one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen"));
    }
}
