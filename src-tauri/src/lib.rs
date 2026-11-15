mod ai;
mod audio;

use ai::engine::{EnhancedResult, FoundryEngine, InsightItem};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_sql::{Builder as Sql, Migration, MigrationKind};
use tokio::sync::RwLock;

pub struct AppState {
    pub cap: Arc<audio::Capture>,
    pub tr: Arc<ai::Transcript>,
    // read-locked by users so transcription and the chat model run side by side; written once when the models load
    pub ai_engine: Arc<RwLock<FoundryEngine>>,
    pub model_error: std::sync::Mutex<Option<String>>,
    pub session: std::sync::Mutex<Option<(audio::Session, Instant)>>,
    // a saved recording is being transcribed again: no live session may start meanwhile
    pub retranscribing: std::sync::atomic::AtomicBool,
}

fn models_loaded(st: &AppState) -> bool {
    // only the loader takes the write lock, briefly, as it installs the ready engine: a busy lock counts as ready
    st.ai_engine.try_read().map(|e| e.is_ready()).unwrap_or(true)
}

#[tauri::command]
fn models_ready(st: State<'_, AppState>) -> Result<bool, String> {
    // a load failure is returned too: the UI may start listening after the one-off model-error event
    match st.model_error.lock().unwrap().clone() {
        Some(e) => Err(e),
        None => Ok(models_loaded(&st)),
    }
}

#[tauri::command]
fn start_syntari_session(app: AppHandle, st: State<'_, AppState>, offset_ms: u64, prior: Vec<ai::engine::TranscriptChunk>, record_session: Option<i64>, mic: Option<String>) -> Result<u64, String> {
    if !models_loaded(&st) {
        return Err("AI models are still loading".into());
    }
    if st.retranscribing.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("A saved recording is being transcribed again; wait for it to finish".into());
    }
    let mut s = st.session.lock().unwrap();
    if s.is_some() {
        return Err("session already running".into());
    }
    // On Continue, seed the previous transcript so notes typed right away still get lookback context
    *st.tr.lock().unwrap() = prior;
    st.cap.pause.reset();
    let rec_dir = match record_session {
        Some(id) => Some(audio::recording::dir(&app.path().app_data_dir().map_err(|e| e.to_string())?, id)),
        None => None,
    };
    // saved audio is the timeline: a continued meeting starts where it ends, so notes, transcript and re-transcribed lines share one clock
    let offset_ms = offset_ms.max(rec_dir.as_deref().map_or(0, |d| audio::recording::end_samples(d) as u64 / 16));
    let t0 = Instant::now();
    let sess = audio::start(app, st.cap.clone(), st.tr.clone(), st.ai_engine.clone(), offset_ms as f64 / 1000.0, rec_dir, mic.filter(|m| !m.is_empty()))?;
    *s = Some((sess, t0));
    Ok(offset_ms)
}

fn downloads_file(name: &str) -> Result<PathBuf, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).map_err(|e| e.to_string())?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    Ok(PathBuf::from(home).join("Downloads").join(name.replace("{secs}", &secs.to_string())))
}

/// Opens a folder, or reveals a file in Finder/Explorer.
fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").args(if path.is_dir() { vec![] } else { vec!["-R"] }).arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer")
        .arg(if path.is_dir() { path.display().to_string() } else { format!("/select,{}", path.display()) })
        .spawn();
}

/// Writes the notes to ~/Downloads and reveals the file in Finder.
#[tauri::command]
fn export_markdown(content: String) -> Result<String, String> {
    let path = downloads_file("syntari-notes-{secs}.md")?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    open_path(&path);
    Ok(path.to_string_lossy().into_owned())
}

/// Destination for a database backup; the frontend writes it with `VACUUM INTO` (a plain file copy would miss WAL data).
/// Bytes of saved audio for a meeting (0: none).
#[tauri::command]
fn recording_size(app: AppHandle, session_id: i64) -> Result<u64, String> {
    Ok(audio::recording::size(&audio::recording::dir(&app.path().app_data_dir().map_err(|e| e.to_string())?, session_id)))
}

#[tauri::command]
fn set_paused(st: State<'_, AppState>, paused: bool) {
    st.cap.pause.set(paused);
}

#[tauri::command]
fn list_mics() -> Vec<String> {
    audio::mic_names()
}

/// Every saved recording: (meeting id, bytes, seconds of audio, days since last written).
#[tauri::command]
fn list_recordings(app: AppHandle) -> Result<Vec<(i64, u64, u64, u64)>, String> {
    Ok(audio::recording::list(&app.path().app_data_dir().map_err(|e| e.to_string())?))
}

/// Deletes saved audio not written for `days` days; returns the bytes freed.
#[tauri::command]
fn delete_old_recordings(app: AppHandle, days: u64) -> Result<u64, String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut freed = 0;
    for (id, bytes, _, age) in audio::recording::list(&base) {
        if age >= days && std::fs::remove_dir_all(audio::recording::dir(&base, id)).is_ok() { freed += bytes; }
    }
    Ok(freed)
}

#[tauri::command]
fn delete_recording(app: AppHandle, session_id: i64) -> Result<(), String> {
    let d = audio::recording::dir(&app.path().app_data_dir().map_err(|e| e.to_string())?, session_id);
    if d.exists() { std::fs::remove_dir_all(d).map_err(|e| e.to_string())?; }
    Ok(())
}

/// Transcribes a meeting's saved audio again; the frontend replaces the transcript with the result.
#[tauri::command]
async fn retranscribe(app: AppHandle, st: State<'_, AppState>, session_id: i64) -> Result<Vec<ai::engine::TranscriptChunk>, String> {
    use std::sync::atomic::Ordering::SeqCst;
    if st.retranscribing.swap(true, SeqCst) { return Err("Already re-transcribing".into()) }
    let result = async {
        if st.session.lock().unwrap().is_some() { return Err("Stop the session first".to_string()) }
        let d = audio::recording::dir(&app.path().app_data_dir().map_err(|e| e.to_string())?, session_id);
        let engine = st.ai_engine.read().await;
        audio::recording::retranscribe(&d, &engine).await
    }.await;
    st.retranscribing.store(false, SeqCst);
    result
}

#[tauri::command]
fn backup_path() -> Result<String, String> {
    Ok(downloads_file("syntari-backup-{secs}.db")?.to_string_lossy().into_owned())
}

#[tauri::command]
fn reveal_path(path: String) {
    open_path(std::path::Path::new(&path));
}

#[tauri::command]
fn open_logs(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all(&dir); // dev builds never create it, and open_path needs it to exist
    open_path(&dir);
    Ok(())
}

/// Release builds append stderr (our `eprintln!`s, panics, Foundry's native logs) to a log file testers can send back.
fn init_logging(dir: &std::path::Path) {
    if cfg!(debug_assertions) { return; }
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("syntari.log");
    if std::fs::metadata(&path).map(|m| m.len() > 5_000_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path); // shortcut: truncate instead of rotating
    }
    let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else { return };
    #[cfg(unix)]
    unsafe { libc::dup2(std::os::fd::AsRawFd::as_raw_fd(&f), 2); }
    #[cfg(windows)]
    unsafe {
        extern "system" { fn SetStdHandle(n: u32, h: *mut core::ffi::c_void) -> i32; }
        SetStdHandle(-12i32 as u32, std::os::windows::io::AsRawHandle::as_raw_handle(&f)); // STD_ERROR_HANDLE
    }
    std::mem::forget(f); // the fd/handle must outlive this function
}

/// A crash mid-transcription can leave a staged chunk behind (see `transcribe_buffer`).
fn remove_stale_audio() {
    for e in std::fs::read_dir(std::env::temp_dir()).into_iter().flatten().flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if n.starts_with("syntari_ephemeral_") && n.ends_with(".wav") { let _ = std::fs::remove_file(e.path()); }
    }
}

#[tauri::command]
async fn stop_syntari_session(st: State<'_, AppState>) -> Result<(), String> {
    let session = st.session.lock().unwrap().take();
    match session {
        // stopping waits for the last transcription; off the main thread so the window stays responsive
        Some((s, _)) => tauri::async_runtime::spawn_blocking(move || s.stop()).await.map_err(|e| e.to_string())?,
        None => Ok(()),
    }
}

#[tauri::command]
fn get_audio_levels(st: State<'_, AppState>) -> audio::Levels {
    st.cap.levels()
}

#[derive(serde::Serialize)]
pub struct PermissionsStatus {
    pub system_audio: bool,
}

#[tauri::command]
fn check_permissions() -> PermissionsStatus {
    #[cfg(target_os = "macos")]
    {
        use screencapturekit::sc_shareable_content::SCShareableContent;
        let res = SCShareableContent::try_current();
        let ok = res.is_ok();
        if let Err(ref e) = res {
            eprintln!("[Syntari Permission] SCShareableContent permission check failed: {e:?}");
        }
        PermissionsStatus { system_audio: ok }
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionsStatus { system_audio: true }
    }
}

#[tauri::command]
fn open_system_settings(pane: Option<String>) {
    #[cfg(target_os = "macos")]
    {
        let url = match pane.as_deref() {
            Some("microphone") => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            _ => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        };
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        // No screen-capture permission on Windows; only the mic toggle matters.
        let _ = pane;
        let _ = std::process::Command::new("cmd").args(["/C", "start", "ms-settings:privacy-microphone"]).spawn();
    }
}

#[tauri::command]
async fn expand_note(
    app: AppHandle,
    text: String,
    ts_ms: u64,
    context: Option<Vec<ai::engine::TranscriptChunk>>,
) -> Result<EnhancedResult, String> {
    let st = app.state::<AppState>();

    // 1. Lookahead: Check without holding the session mutex lock during sleep
    let is_running = st.session.lock().unwrap().is_some();
    if is_running {
        tokio::time::sleep(std::time::Duration::from_millis(ai::LOOKAHEAD_MS)).await;
    }

    // 2. Temporal context slice [T - 45s, T + 10s]
    // A past meeting opened for review has no live transcript in Rust, so the UI sends the stored one.
    let window_chunks = match context {
        Some(all) if !is_running => {
            let (t_min, t_max) = ai::calculate_window_bounds(ts_ms);
            all.into_iter().filter(|c| c.end >= t_min && c.start <= t_max).collect()
        }
        _ => ai::get_window_chunks(&st.tr, ts_ms),
    };
    let keystroke_secs = ai::ms_to_secs(ts_ms);

    // 3. Perform inference
    let engine = st.ai_engine.read().await;
    engine.expand_shorthand(&text, keystroke_secs, &window_chunks).await
}

/// Extracts from the chunks after index `from` and returns the next index. An index, not a time:
/// chunks are stamped with their audio start but arrive seconds later, so a time cursor skipped late ones.
#[tauri::command]
async fn extract_insights(app: AppHandle, from: usize) -> Result<(Vec<InsightItem>, usize), String> {
    let st = app.state::<AppState>();
    let chunks: Vec<_> = st.tr.lock().unwrap().iter().skip(from).cloned().collect();
    let engine = st.ai_engine.read().await;
    Ok((engine.extract_insights(&chunks).await?, from + chunks.len()))
}

#[tauri::command]
async fn summarize_transcript(app: AppHandle, chunks: Vec<ai::engine::TranscriptChunk>) -> Result<String, String> {
    let st = app.state::<AppState>();
    let engine = st.ai_engine.read().await;
    engine.summarize(&chunks, |i, n| { let _ = app.emit("summarize-progress", serde_json::json!({ "part": i, "total": n })); }).await
}

/// Answer from passages the frontend retrieved; each is (number, "meeting, time: text").
#[tauri::command]
async fn ask_meetings(app: AppHandle, question: String, passages: Vec<(usize, String)>) -> Result<String, String> {
    let st = app.state::<AppState>();
    let engine = st.ai_engine.read().await;
    engine.answer(&question, &passages).await
}

pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "init",
        sql: include_str!("../migrations/001_init.sql"),
        kind: MigrationKind::Up,
    }, Migration {
        version: 2,
        description: "summary",
        sql: include_str!("../migrations/002_summary.sql"),
        kind: MigrationKind::Up,
    }, Migration {
        version: 3,
        description: "title",
        sql: include_str!("../migrations/003_title.sql"),
        kind: MigrationKind::Up,
    }, Migration {
        version: 4,
        description: "citation_start",
        sql: include_str!("../migrations/004_citation_start.sql"),
        kind: MigrationKind::Up,
    }, Migration {
        version: 5,
        description: "transcript_excluded",
        sql: include_str!("../migrations/005_transcript_excluded.sql"),
        kind: MigrationKind::Up,
    }, Migration {
        version: 6,
        description: "summary_stale",
        sql: include_str!("../migrations/006_summary_stale.sql"),
        kind: MigrationKind::Up,
    }, Migration {
        version: 7,
        description: "search_index",
        sql: include_str!("../migrations/007_search_index.sql"),
        kind: MigrationKind::Up,
    }];

    // Plain on-device SQLite: the plugin's sqlx rejects a `?key=` URL param, so a keychain-keyed URL never opened.
    tauri::Builder::default()
        .plugin(Sql::default().add_migrations("sqlite:syntari.db", migrations).build())
        .setup(|app| {
            if let Ok(dir) = app.path().app_log_dir() { init_logging(&dir); }
            remove_stale_audio();
            let resource_path = std::env::var("SYNTARI_MODELS_DIR").map(PathBuf::from).unwrap_or_else(|_| {
                app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from(".")).join("models")
            });

            let engine = Arc::new(RwLock::new(FoundryEngine::new()));
            // managed before the loader starts, so the loader can record a failure in it
            app.manage(AppState {
                cap: Arc::default(),
                tr: Arc::default(),
                ai_engine: Arc::clone(&engine),
                model_error: Default::default(),
                session: std::sync::Mutex::new(None),
                retranscribing: Default::default(),
            });
            let handle = app.handle().clone();

            // Asynchronously initialize local models in background
            tauri::async_runtime::spawn(async move {
                let h = handle.clone();
                let progress: ai::engine::Progress =
                    Arc::new(move |name, pct| { let _ = h.emit("model-progress", serde_json::json!({ "name": name, "percent": pct })); });
                match FoundryEngine::load(resource_path, progress).await {
                    Err(e) => {
                        eprintln!("[Syntari AI Init Error]: {}", e);
                        *handle.state::<AppState>().model_error.lock().unwrap() = Some(e.clone());
                        let _ = handle.emit("model-error", e);
                    }
                    Ok(loaded) => {
                        *engine.write().await = loaded;
                        let _ = handle.emit("models-ready", ());
                        println!("[Syntari AI]: Offline Foundry Local models loaded successfully.");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            models_ready,
            start_syntari_session,
            stop_syntari_session,
            get_audio_levels,
            check_permissions,
            open_system_settings,
            expand_note,
            export_markdown,
            backup_path,
            recording_size,
            set_paused,
            list_mics,
            list_recordings,
            delete_old_recordings,
            delete_recording,
            retranscribe,
            reveal_path,
            open_logs,
            extract_insights,
            summarize_transcript,
            ask_meetings
        ])
        .build(tauri::generate_context!())
        .expect("error while building Syntari application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some((sess, _)) = app.state::<AppState>().session.lock().unwrap().take() {
                    let _ = sess.stop();
                }
                // Foundry's native Manager aborts (SIGABRT in SpdlogLogger dtor) when C++ static destructors
                // run at normal process exit; skip them. shortcut: revisit when Foundry fixes its teardown.
                #[cfg(unix)]
                unsafe { libc::_exit(0) }
                #[cfg(windows)]
                unsafe {
                    extern "system" {
                        fn GetCurrentProcess() -> *mut core::ffi::c_void;
                        fn TerminateProcess(h: *mut core::ffi::c_void, code: u32) -> i32;
                    }
                    TerminateProcess(GetCurrentProcess(), 0); // skips DLL/static teardown, like _exit
                }
            }
        });
}
