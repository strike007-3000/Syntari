mod ai;
mod audio;

use ai::engine::{EnhancedResult, FoundryEngine};
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

#[tauri::command]
fn start_syntari_session(app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    let mut s = st.session.lock().unwrap();
    if s.is_some() {
        return Err("session already running".into());
    }
    st.tr.lock().unwrap().clear();
    let t0 = Instant::now();
    let sess = audio::start(app, st.cap.clone(), st.tr.clone(), st.ai_engine.clone(), t0)?;
    *s = Some((sess, t0));
    Ok(())
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

pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "init",
        sql: include_str!("../migrations/001_init.sql"),
        kind: MigrationKind::Up,
    }];

    tauri::Builder::default()
        .plugin(Sql::default().add_migrations("sqlite:syntari.db", migrations).build())
        .setup(|app| {
            let resource_path = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("resources/models");

            let engine = Arc::new(AsyncMutex::new(FoundryEngine::new()));
            let engine_clone = Arc::clone(&engine);

            // Asynchronously initialize local models in background
            tauri::async_runtime::spawn(async move {
                let mut guard = engine_clone.lock().await;
                if let Err(e) = guard.initialize(resource_path).await {
                    eprintln!("[Syntari AI Init Error]: {}", e);
                } else {
                    println!("[Syntari AI]: Offline Foundry Local models loaded successfully.");
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
            start_syntari_session,
            stop_syntari_session,
            get_audio_levels,
            check_permissions,
            open_system_settings,
            expand_note
        ])
        .run(tauri::generate_context!())
        .expect("error while running Syntari application");
}
