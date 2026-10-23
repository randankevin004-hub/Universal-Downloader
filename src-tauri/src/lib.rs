use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    active_pid: Mutex<Option<u32>>,
    cancelled: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolStatus {
    yt_dlp: Option<String>,
    existing_yt_dlp: Option<String>,
    ffmpeg: Option<String>,
    yt_dlp_managed: bool,
    ffmpeg_managed: bool,
    version: Option<String>,
    download_folder: String,
}

#[derive(Clone, Serialize)]
struct SetupProgress {
    message: String,
    percent: u8,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    percent: f32,
    message: String,
}

#[derive(Clone, Serialize)]
struct DownloadComplete {
    ok: bool,
    cancelled: bool,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadResult {
    ok: bool,
    cancelled: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

fn tools_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("tools");
    fs::create_dir_all(&path).map_err(|e| format!("Could not create the app tools folder: {e}"))?;
    Ok(path)
}

fn managed_yt_dlp(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(tools_dir(app)?.join("yt-dlp.exe"))
}

fn app_download_folder(_app: &AppHandle) -> String {
    std::env::var("USERPROFILE")
        .map(|profile| PathBuf::from(profile).join("Downloads").to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

fn where_tool(name: &str) -> Option<String> {
    let output = Command::new("where.exe").arg(name).creation_flags(0x08000000).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

fn tool_version(path: Option<&str>) -> Option<String> {
    let output = Command::new(path?).arg("--version").creation_flags(0x08000000).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn current_status(app: &AppHandle) -> Result<ToolStatus, String> {
    let local = managed_yt_dlp(app)?;
    let managed = local.exists().then(|| local.to_string_lossy().into_owned());
    let existing = where_tool("yt-dlp.exe");
    let selected = managed.clone().or_else(|| existing.clone());
    let ffmpeg_managed_path = tools_dir(app)?.join("ffmpeg.exe");
    let ffmpeg_managed = ffmpeg_managed_path.exists();
    let ffmpeg = if ffmpeg_managed {
        Some(ffmpeg_managed_path.to_string_lossy().into_owned())
    } else {
        where_tool("ffmpeg.exe")
    };
    Ok(ToolStatus {
        version: tool_version(selected.as_deref()),
        yt_dlp: selected,
        existing_yt_dlp: existing,
        ffmpeg,
        yt_dlp_managed: managed.is_some(),
        ffmpeg_managed,
        download_folder: app_download_folder(app),
    })
}

#[tauri::command]
fn get_status(app: AppHandle) -> Result<ToolStatus, String> {
    current_status(&app)
}

#[tauri::command]
async fn install_ytdlp(app: AppHandle) -> Result<ToolStatus, String> {
    let managed = managed_yt_dlp(&app)?;
    if managed.exists() || where_tool("yt-dlp.exe").is_some() {
        let status = current_status(&app)?;
        let source = if status.yt_dlp_managed { "the app's saved copy" } else { "the existing copy on your PC" };
        let _ = app.emit("setup-progress", SetupProgress {
            message: format!("Using {}{}.", source, status.version.as_deref().map(|v| format!(" (yt-dlp {v})")).unwrap_or_default()),
            percent: 100,
        });
        return Ok(status);
    }
    let _ = app.emit("setup-progress", SetupProgress { message: "Checking the official yt-dlp release…".into(), percent: 5 });
    let client = reqwest::Client::builder()
        .user_agent("UniversalDownloader/1.0")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Could not prepare the secure download: {e}"))?;
    let release: GithubRelease = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Could not reach the official yt-dlp release: {e}"))?
        .error_for_status()
        .map_err(|e| format!("The official release returned an error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Could not read the official release information: {e}"))?;
    let asset = release.assets.iter().find(|asset| asset.name == "yt-dlp.exe")
        .ok_or_else(|| "The official release did not include its Windows executable.".to_string())?;
    let _ = app.emit("setup-progress", SetupProgress { message: format!("Downloading yt-dlp {}…", release.tag_name), percent: 20 });
    let response = client.get(&asset.browser_download_url).send().await
        .map_err(|e| format!("Could not download yt-dlp: {e}"))?
        .error_for_status().map_err(|e| format!("The yt-dlp download returned an error: {e}"))?;
    let bytes = response.bytes().await.map_err(|e| format!("Could not finish downloading yt-dlp: {e}"))?;
    if let Some(expected) = asset.digest.as_deref().and_then(|digest| digest.strip_prefix("sha256:")) {
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("The downloaded file failed its SHA-256 check and was not installed.".into());
        }
    }
    let destination = managed_yt_dlp(&app)?;
    let temporary = destination.with_extension("exe.download");
    fs::write(&temporary, &bytes).map_err(|e| format!("Could not save the yt-dlp download: {e}"))?;
    if destination.exists() {
        fs::remove_file(&destination).map_err(|e| format!("Could not replace the existing app copy: {e}"))?;
    }
    fs::rename(&temporary, &destination).map_err(|e| format!("Could not finish installing yt-dlp: {e}"))?;
    let _ = app.emit("setup-progress", SetupProgress { message: format!("yt-dlp {} is ready.", release.tag_name), percent: 100 });
    current_status(&app)
}

fn quality_format(quality: &str, has_ffmpeg: bool) -> Result<String, String> {
    if quality == "best" {
        return Ok(if has_ffmpeg { "bestvideo*+bestaudio/best" } else { "best" }.into());
    }
    let cap = match quality { "1080" => 1080, "720" => 720, "480" => 480, "360" => 360, _ => return Err("Choose one of the listed quality options.".into()) };
    Ok(if has_ffmpeg {
        format!("bestvideo*[height<={cap}]+bestaudio/best[height<={cap}]")
    } else {
        format!("best[height<={cap}]")
    })
}

fn parse_line(app: &AppHandle, line: &str) {
    if let Some(download) = line.find("[download]") {
        let rest = line[download + "[download]".len()..].trim();
        if let Some(token) = rest.split_whitespace().next() {
            if let Some(percent) = token.strip_suffix('%').and_then(|value| value.parse::<f32>().ok()) {
                let _ = app.emit("download-progress", DownloadProgress { percent, message: line.trim().to_string() });
                return;
            }
        }
    }
    if line.contains("[download]") || line.contains("[Merger]") || line.contains("ERROR") || line.contains("WARNING") {
        let _ = app.emit("download-message", line.trim().to_string());
    }
}

fn pipe_lines<R: Read + Send + 'static>(stream: R, sender: mpsc::Sender<String>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().flatten() {
            if sender.send(line).is_err() { break; }
        }
    });
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    quality: String,
    folder: Option<String>,
) -> DownloadResult {
    let parsed = match Url::parse(&url) {
        Ok(url) if url.scheme() == "http" || url.scheme() == "https" => url,
        _ => return DownloadResult { ok: false, cancelled: false, error: Some("Enter a valid web link starting with http or https.".into()) },
    };
    let tools = match current_status(&app) {
        Ok(tools) => tools,
        Err(error) => return DownloadResult { ok: false, cancelled: false, error: Some(error) },
    };
    let executable = match tools.yt_dlp.as_deref() {
        Some(path) => PathBuf::from(path),
        None => return DownloadResult {
            ok: false,
            cancelled: false,
            error: Some("No yt-dlp was found. Add yt-dlp.exe to PATH or set it up from this app.".into()),
        },
    };
    let format = match quality_format(&quality, tools.ffmpeg.is_some()) {
        Ok(format) => format,
        Err(error) => return DownloadResult { ok: false, cancelled: false, error: Some(error) },
    };
    let destination = PathBuf::from(folder.unwrap_or(tools.download_folder));
    if let Err(error) = fs::create_dir_all(&destination) {
        return DownloadResult { ok: false, cancelled: false, error: Some(format!("Could not create the save folder: {error}")) };
    }
    {
        let mut active = state.active_pid.lock().unwrap();
        if active.is_some() {
            return DownloadResult { ok: false, cancelled: false, error: Some("A download is already running.".into()) };
        }
        state.cancelled.store(false, Ordering::SeqCst);
        *active = Some(0);
    }
    let mut command = Command::new(executable);
    command.args([
        "--newline", "--progress", "--no-playlist", "--no-warnings", "-f", &format,
        "--merge-output-format", "mp4", "-P", destination.to_string_lossy().as_ref(),
        "-o", "%(title).200B [%(id)s].%(ext)s",
    ]).current_dir(&destination).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(0x08000000);
    if let Some(ffmpeg) = tools.ffmpeg.as_deref() { command.arg("--ffmpeg-location").arg(ffmpeg); }
    // `--` ends option parsing. Keep it immediately before the media URL so
    // Windows paths passed to --ffmpeg-location are not mistaken for URLs.
    command.arg("--").arg(parsed.as_str());
    let mut child: Child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            *state.active_pid.lock().unwrap() = None;
            return DownloadResult { ok: false, cancelled: false, error: Some(format!("Could not start yt-dlp: {error}")) };
        }
    };
    let pid = child.id();
    *state.active_pid.lock().unwrap() = Some(pid);
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() { pipe_lines(stdout, sender.clone()); }
    if let Some(stderr) = child.stderr.take() { pipe_lines(stderr, sender.clone()); }
    drop(sender);
    let _ = app.emit("download-progress", DownloadProgress { percent: 0.0, message: "Starting download…".into() });
    let mut status = None;
    loop {
        for line in receiver.try_iter() { parse_line(&app, &line); }
        match child.try_wait() {
            Ok(Some(exit)) => { status = Some(exit); break; }
            Ok(None) => thread::sleep(Duration::from_millis(80)),
            Err(_) => break,
        }
    }
    while let Ok(line) = receiver.recv_timeout(Duration::from_millis(30)) { parse_line(&app, &line); }
    *state.active_pid.lock().unwrap() = None;
    let cancelled = state.cancelled.swap(false, Ordering::SeqCst);
    let ok = status.map(|status| status.success()).unwrap_or(false);
    let message = if cancelled { "Download cancelled." } else if ok { "Download finished." } else { "The download could not be completed." };
    let _ = app.emit("download-complete", DownloadComplete { ok, cancelled, message: message.into() });
    DownloadResult {
        ok,
        cancelled,
        error: if ok || cancelled { None } else { Some(format!("yt-dlp exited with code {}.", status.and_then(|s| s.code()).unwrap_or(-1))) },
    }
}

#[tauri::command]
fn cancel_download(state: State<'_, AppState>) -> Result<bool, String> {
    let pid = *state.active_pid.lock().map_err(|_| "Could not read the active download state.")?;
    let Some(pid) = pid.filter(|pid| *pid > 0) else { return Ok(false); };
    state.cancelled.store(true, Ordering::SeqCst);
    let _ = Command::new("taskkill.exe").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(0x08000000).output();
    Ok(true)
}

#[tauri::command]
fn open_folder(folder: Option<String>, app: AppHandle) -> Result<(), String> {
    let folder = folder.unwrap_or_else(|| app_download_folder(&app));
    Command::new("explorer.exe").arg(folder).spawn().map(|_| ()).map_err(|e| format!("Could not open the save folder: {e}"))
}

#[tauri::command]
fn open_ffmpeg_page() -> Result<(), String> {
    Command::new("cmd.exe").args(["/c", "start", "", "https://www.gyan.dev/ffmpeg/builds/"]).spawn()
        .map(|_| ()).map_err(|e| format!("Could not open the FFmpeg builds page: {e}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState { active_pid: Mutex::new(None), cancelled: AtomicBool::new(false) })
        .invoke_handler(tauri::generate_handler![get_status, install_ytdlp, start_download, cancel_download, open_folder, open_ffmpeg_page])
        .run(tauri::generate_context!())
        .expect("error while running Universal Downloader");
}
