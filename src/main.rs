//! Singlish Caption Studio: Sinhala and English auto-captions for CapCut.
//! The UI can only call the commands below, and it never supplies a file path.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod audio;
mod draft;
mod eleven;
mod fonts;
mod pipeline;
mod secrets;
mod srt;
mod store;
mod text;
mod updater;
mod usage;
mod validate;

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use text::GlossaryData;
use validate::{check_settings, project_name_ok, Settings};

static JOB_RUNNING: AtomicBool = AtomicBool::new(false);
const MAX_CLIPS: usize = 300;

/// CapCut segment ids look like UUIDs; accept nothing else from the UI.
fn segment_id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Clears the "job running" flag however the job thread ends.
struct JobGuard;
impl Drop for JobGuard {
    fn drop(&mut self) {
        JOB_RUNNING.store(false, Ordering::SeqCst);
    }
}

type Res<T> = Result<T, String>;

/// Run blocking work off the UI thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "Task failed.".to_string())?
}

#[derive(Serialize, Clone)]
struct ProjectEntry {
    folder: String,
    title: String,
    modified: i64,
}

fn read_projects() -> Res<Vec<ProjectEntry>> {
    let root = store::drafts_root()?;
    let text = store::read_capped(&root.join("root_meta_info.json"), 16 * 1024 * 1024)
        .ok_or("Could not read CapCut's project list.")?;
    let reg: Value =
        serde_json::from_str(&text).map_err(|_| "CapCut's project list is not readable.")?;
    let mut out: Vec<ProjectEntry> = reg["all_draft_store"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let fold = e["draft_fold_path"].as_str()?;
            let folder = fold.rsplit(['/', '\\']).next()?.to_string();
            if !project_name_ok(&folder) || !root.join(&folder).join("draft_content.json").is_file()
            {
                return None;
            }
            let title = e["draft_name"]
                .as_str()
                .filter(|t| !t.is_empty())
                .unwrap_or(&folder);
            Some(ProjectEntry {
                title: title.chars().filter(|c| !c.is_control()).take(80).collect(),
                modified: e["tm_draft_modified"].as_i64().unwrap_or(0),
                folder,
            })
        })
        .take(200)
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.modified));
    Ok(out)
}

/// Resolve a UI-supplied folder name to a real project path and its title, only if CapCut lists it.
fn project_entry(folder: &str) -> Res<(PathBuf, String)> {
    if !project_name_ok(folder) {
        return Err("Invalid project name.".into());
    }
    let title = read_projects()?
        .into_iter()
        .find(|p| p.folder == folder)
        .map(|p| p.title)
        .ok_or("That project is not in CapCut's list.")?;
    Ok((store::drafts_root()?.join(folder), title))
}

fn project_path(folder: &str) -> Res<PathBuf> {
    project_entry(folder).map(|(path, _)| path)
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

#[tauri::command]
async fn list_projects() -> Res<Vec<ProjectEntry>> {
    blocking(read_projects).await
}

#[tauri::command]
async fn inspect_project(folder: String) -> Res<Value> {
    blocking(move || {
        let (path, title) = project_entry(&folder)?;
        let info = draft::inspect(&path)?;
        let srt_file = srt::path_for(&title).map(|p| p.is_file()).unwrap_or(false);
        // Only what the UI needs: names, times and ids. No absolute paths leave the backend.
        let clips: Vec<Value> = info
            .clips
            .iter()
            .take(MAX_CLIPS)
            .map(|c| {
                json!({
                    "id": clean(&c.id, 64),
                    "name": clean(&c.name, 120),
                    "start": c.tl_start,
                    "dur": c.tl_dur,
                    "audio_s": c.src_dur,
                    "volume": c.volume,
                    "ok": c.file_ok,
                    "selected": c.default_selected(),
                })
            })
            .collect();
        Ok(json!({
            "capcut_running": info.capcut_running,
            "duration_s": info.duration_s,
            "has_captions": info.has_captions,
            "srt_file": srt_file,
            "clips": clips,
            "problems": info.problems.iter().take(5).map(|p| clean(p, 300)).collect::<Vec<_>>(),
            "warnings": info.warnings.iter().take(5).map(|p| clean(p, 300)).collect::<Vec<_>>(),
        }))
    })
    .await
}

#[derive(Serialize)]
struct FontEntry {
    name: String,
    collection: bool,
}

#[tauri::command]
async fn list_fonts() -> Res<Vec<FontEntry>> {
    blocking(|| {
        Ok(fonts::installed()
            .into_iter()
            .take(3000)
            .map(|f| FontEntry {
                name: f.name,
                collection: f.collection,
            })
            .collect())
    })
    .await
}

#[tauri::command]
async fn get_settings() -> Res<Settings> {
    blocking(|| Ok(store::load_config(&fonts::installed()).style)).await
}

#[tauri::command]
async fn save_settings(settings: Settings) -> Res<()> {
    blocking(move || {
        let installed = fonts::installed();
        let names: Vec<String> = installed.iter().map(|f| f.name.clone()).collect();
        check_settings(&settings, &names)?;
        let mut cfg = store::load_config(&installed);
        cfg.style = settings;
        store::save_config(&cfg)
    })
    .await
}

#[tauri::command]
async fn setup_state() -> Res<Value> {
    blocking(|| {
        let (installed, version, tested) = store::capcut_status();
        let fonts = fonts::installed();
        let cfg = store::load_config(&fonts);
        Ok(json!({
            "capcut": {"installed": installed, "version": version, "tested": tested},
            "key_set": secrets::exists(),
            "setup_done": cfg.setup_done,
            "fonts_ok": fonts::find(&fonts, &cfg.style.sinhala.font).is_some() && fonts::find(&fonts, &cfg.style.english.font).is_some(),
        }))
    })
    .await
}

#[tauri::command]
async fn complete_setup() -> Res<()> {
    blocking(|| {
        let mut cfg = store::load_config(&fonts::installed());
        cfg.setup_done = true;
        store::save_config(&cfg)
    })
    .await
}

#[tauri::command]
async fn key_status() -> Res<bool> {
    blocking(|| Ok(secrets::exists())).await
}

/// Validates the key with ElevenLabs (free check) and only then stores it.
#[tauri::command]
async fn save_api_key(key: String) -> Res<()> {
    blocking(move || {
        let key = key.trim().to_string();
        if !secrets::key_format_ok(&key) {
            return Err("That does not look like an ElevenLabs key. Copy the whole key from your ElevenLabs account.".into());
        }
        eleven::check_key(&key)?;
        secrets::set(&key)
    })
    .await
}

#[tauri::command]
async fn delete_api_key() -> Res<()> {
    blocking(secrets::delete).await
}

#[tauri::command]
async fn get_glossary() -> Res<GlossaryData> {
    blocking(|| Ok(store::load_glossary())).await
}

#[tauri::command]
async fn save_glossary(glossary: GlossaryData) -> Res<()> {
    blocking(move || store::save_glossary(&glossary)).await
}

#[tauri::command]
async fn restore_backup(folder: String) -> Res<usize> {
    let path = blocking(move || project_path(&folder)).await?;
    // Share the lock with caption runs, so two jobs never edit a project at the same time.
    if JOB_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A caption job is running. Wait for it to finish.".into());
    }
    let _guard = JobGuard;
    blocking(move || draft::restore_latest_backup(&path)).await
}

#[tauri::command]
async fn get_usage() -> Res<Value> {
    blocking(|| {
        let mut summary = usage::load().summary();
        let price = store::load_config(&fonts::installed()).price_per_minute;
        summary["price_per_minute"] = json!(price);
        Ok(summary)
    })
    .await
}

#[tauri::command]
async fn clear_usage() -> Res<()> {
    blocking(usage::clear).await
}

/// `None` clears the price. The number is only ever used to show an estimate.
#[tauri::command]
async fn set_price(price: Option<f64>) -> Res<()> {
    blocking(move || {
        if price.is_some_and(|p| !p.is_finite() || !(0.0..=1000.0).contains(&p)) {
            return Err("The price must be a number between 0 and 1000.".into());
        }
        let mut cfg = store::load_config(&fonts::installed());
        cfg.price_per_minute = price;
        store::save_config(&cfg)
    })
    .await
}

#[tauri::command]
fn app_info() -> Value {
    json!({
        "name": "Singlish Caption Studio",
        "version": env!("CARGO_PKG_VERSION"),
        "license": "MIT",
        "developer": "Wasath",
    })
}

/// The UI asks for a link by name. The addresses live here, so the page cannot open anything else.
#[tauri::command]
async fn open_link(which: String) -> Res<()> {
    let url = match which.as_str() {
        "coffee" => "https://buymeacoffee.com/wasath",
        "elevenlabs_keys" => "https://elevenlabs.io/app/settings/api-keys",
        _ => return Err("Unknown link.".into()),
    };
    blocking(move || {
        let sys = std::env::var_os("SYSTEMROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let mut cmd = std::process::Command::new(sys.join("System32").join("rundll32.exe"));
        cmd.args(["url.dll,FileProtocolHandler", url]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|_| "Could not open your browser.".to_string())
    })
    .await
}

/// Show the project's caption file in Explorer. The path is built here from the project's title,
/// so the page cannot point Explorer at anything else.
#[tauri::command]
async fn reveal_srt(folder: String) -> Res<()> {
    blocking(move || {
        let (_, title) = project_entry(&folder)?;
        let file = srt::path_for(&title)?;
        if !file.is_file() {
            return Err("There is no caption file for this project yet.".into());
        }
        let sys = std::env::var_os("SYSTEMROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let mut cmd = std::process::Command::new(sys.join("explorer.exe"));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // File names cannot contain quotes, so quoting the whole path is safe.
            cmd.raw_arg(format!("/select,\"{}\"", file.display()));
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|_| "Could not open the folder.".to_string())
    })
    .await
}

#[tauri::command]
async fn check_update(app: AppHandle, pending: tauri::State<'_, updater::Pending>) -> Res<Value> {
    updater::check(&app, &pending).await
}

#[tauri::command]
async fn install_update(app: AppHandle, pending: tauri::State<'_, updater::Pending>) -> Res<()> {
    // Never replace the app in the middle of a caption run.
    if JOB_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A caption job is running. Wait for it to finish, then update.".into());
    }
    let _guard = JobGuard;
    updater::install(&app, &pending).await
}

#[tauri::command]
async fn get_update_settings() -> Res<Value> {
    blocking(|| {
        let cfg = store::load_config(&fonts::installed());
        Ok(json!({"auto_check": cfg.auto_update_check, "version": env!("CARGO_PKG_VERSION")}))
    })
    .await
}

#[tauri::command]
async fn set_auto_update_check(enabled: bool) -> Res<()> {
    blocking(move || {
        let mut cfg = store::load_config(&fonts::installed());
        cfg.auto_update_check = enabled;
        store::save_config(&cfg)
    })
    .await
}

#[tauri::command]
async fn start_caption(
    app: AppHandle,
    folder: String,
    allow_cloud: bool,
    clips: Vec<String>,
) -> Res<()> {
    if clips.len() > MAX_CLIPS || !clips.iter().all(|id| segment_id_ok(id)) {
        return Err("Invalid clip selection.".into());
    }
    let (path, title) = blocking(move || project_entry(&folder)).await?;
    if JOB_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A caption job is already running.".into());
    }
    thread::spawn(move || {
        let _guard = JobGuard;
        pipeline::run(&path, &title, allow_cloud, Some(&clips), &|ev| {
            let _ = app.emit("caption-event", ev);
        });
    });
    Ok(())
}

/// The window may only show the app's own page. Anything else is blocked.
fn navigation_allowed(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::Pending::default())
        .setup(|app| {
            let cfg = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("no window is configured")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &cfg)?
                .on_navigation(navigation_allowed)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            inspect_project,
            list_fonts,
            get_settings,
            save_settings,
            setup_state,
            complete_setup,
            key_status,
            save_api_key,
            delete_api_key,
            get_glossary,
            save_glossary,
            restore_backup,
            get_usage,
            clear_usage,
            set_price,
            app_info,
            open_link,
            check_update,
            install_update,
            get_update_settings,
            set_auto_update_check,
            reveal_srt,
            start_caption
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("Singlish Caption Studio failed to start: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_own_page_may_be_shown() {
        for ok in [
            "http://tauri.localhost/",
            "https://tauri.localhost/index.html",
            "tauri://localhost/",
        ] {
            assert!(navigation_allowed(&tauri::Url::parse(ok).unwrap()), "{ok}");
        }
        for bad in [
            "https://evil.example/",
            "http://tauri.localhost.evil.example/",
            "http://localhost:8080/",
            "file:///C:/Windows/win.ini",
            "javascript:alert(1)",
            "data:text/html,hi",
        ] {
            assert!(
                !navigation_allowed(&tauri::Url::parse(bad).unwrap()),
                "{bad}"
            );
        }
    }

    #[test]
    fn clip_ids_are_checked() {
        assert!(segment_id_ok("D1AF8169-B962-4e46-A37C-BEAD56D35555"));
        for bad in ["", "a b", "x/y", "..", &"a".repeat(65), "a\u{0}b"] {
            assert!(!segment_id_ok(bad), "{bad:?}");
        }
    }
}
