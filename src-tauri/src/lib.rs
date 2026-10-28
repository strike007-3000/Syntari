mod ai;
mod audio;

use ai::engine::{EnhancedResult, FoundryEngine, InsightItem};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_sql::{Builder as Sql, Migration, MigrationKind};
use tokio::sync::Mutex as AsyncMutex;

pub struct AppState {
    pub cap: Arc<audio::Capture>,
    pub tr: Arc<ai::Transcript>,
    pub ai_engine: Arc<AsyncMutex<FoundryEngine>>,
    pub session: std::sync::Mutex<Option<(audio::Session, Instant)>>,
}

fn models_loaded(st: &AppState) -> bool {
    // try_lock: the engine is only locked briefly by in-flight work, so a busy lock counts as ready
    st.ai_engine.try_lock().map(|e| e.is_ready()).unwrap_or(true)
}

#[tauri::command]
fn models_ready(st: State<'_, AppState>) -> bool {
    models_loaded(&st)
}

#[tauri::command]
fn start_syntari_session(app: AppHandle, st: State<'_, AppState>, offset_ms: u64) -> Result<(), String> {
    if !models_loaded(&st) {
        return Err("AI models are still loading".into());
    }
    let mut s = st.session.lock().unwrap();
    if s.is_some() {
        return Err("session already running".into());
    }
    st.tr.lock().unwrap().clear();
    let t0 = Instant::now();
    let sess = audio::start(app, st.cap.clone(), st.tr.clone(), st.ai_engine.clone(), offset_ms as f64 / 1000.0)?;
    *s = Some((sess, t0));
    Ok(())
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
    open_path(&app.path().app_log_dir().map_err(|e| e.to_string())?);
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
fn stop_syntari_session(st: State<'_, AppState>) -> Result<(), String> {
    match st.session.lock().unwrap().take() {
        Some((s, _)) => s.stop(),
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
    id: String,
    _kind: String,
    text: String,
    ts_ms: u64,
) -> Result<EnhancedResult, String> {
    let st = app.state::<AppState>();

    // 1. Lookahead: Check without holding the session mutex lock during sleep
    let is_running = st.session.lock().unwrap().is_some();
    if is_running {
        tokio::time::sleep(std::time::Duration::from_millis(ai::LOOKAHEAD_MS)).await;
    }

    // 2. Temporal context slice [T - 45s, T + 10s]
    let window_chunks = ai::get_window_chunks(&st.tr, ts_ms);
    let keystroke_secs = ai::ms_to_secs(ts_ms);

    // 3. Perform inference
    let engine = st.ai_engine.lock().await;
    let result = engine
        .expand_shorthand(&text, keystroke_secs, &window_chunks)
        .await
        .map_err(|e| e.to_string())?;

    // 4. Emit event for listeners
    use tauri::Emitter;
    let _ = app.emit(
        "note-expanded",
        serde_json::json!({
            "id": id,
            "category": result.category,
            "owner": result.owner,
            "due": result.due,
            "enhanced_text": result.enhanced_text,
            "raw_shorthand": result.raw_shorthand,
            "citation_start": result.citation_start,
            "citation_end": result.citation_end,
            "citation_quote": result.citation_quote
        }),
    );

    // 5. Also directly return to caller for immediate resolution
    Ok(result)
}

#[tauri::command]
async fn extract_insights(app: AppHandle, since_ms: u64) -> Result<Vec<InsightItem>, String> {
    let st = app.state::<AppState>();
    let chunks: Vec<_> = st
        .tr
        .lock()
        .unwrap()
        .iter()
        .filter(|c| (c.start * 1000.0) as u64 >= since_ms)
        .cloned()
        .collect();
    let engine = st.ai_engine.lock().await;
    engine.extract_insights(&chunks).await
}

#[tauri::command]
async fn summarize_transcript(app: AppHandle, chunks: Vec<ai::engine::TranscriptChunk>) -> Result<String, String> {
    let st = app.state::<AppState>();
    let engine = st.ai_engine.lock().await;
    engine.summarize(&chunks).await
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

            let engine = Arc::new(AsyncMutex::new(FoundryEngine::new()));
            let engine_clone = Arc::clone(&engine);
            let handle = app.handle().clone();

            // Asynchronously initialize local models in background
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                let h = handle.clone();
                let progress: ai::engine::Progress =
                    Arc::new(move |name, pct| { let _ = h.emit("model-progress", serde_json::json!({ "name": name, "percent": pct })); });
                match FoundryEngine::load(resource_path, progress).await {
                    Err(e) => {
                        eprintln!("[Syntari AI Init Error]: {}", e);
                        let _ = handle.emit("model-error", e);
                    }
                    Ok(loaded) => {
                    *engine_clone.lock().await = loaded;
                    let _ = handle.emit("models-ready", ());
                    println!("[Syntari AI]: Offline Foundry Local models loaded successfully.");
                    }
                }
            });

            app.manage(AppState {
                cap: Arc::default(),
                tr: Arc::default(),
                ai_engine: engine,
                session: std::sync::Mutex::new(None),
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
            reveal_path,
            open_logs,
            extract_insights,
            summarize_transcript
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
