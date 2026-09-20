//! Local support reports. The attachment is captured once for preview and
//! passed unchanged into the ZIP; saving never reads additional logs.
use std::io::{Read, Seek, SeekFrom, Write};
use std::fmt::Write as _;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};
use crate::error::{AppError, AppResult};

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn open_report_window(app: tauri::AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("report") {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(&app, "report", WebviewUrl::App("index.html".into()))
        .initialization_script("if (!location.hash) location.hash = '#report';")
        .title("Report bug");
    let builder = match crate::browser_args() {
        Some(args) => builder.additional_browser_args(&args),
        None => builder,
    };
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    let window = builder.inner_size(660.0, 650.0).min_inner_size(480.0, 440.0)
        .build().map_err(|e| AppError::internal(format!("The report window could not open: {e}")))?;
    let _ = window.hide_menu();
    Ok(())
}

#[tauri::command]
pub async fn report_attachment(player: State<'_, std::sync::Arc<crate::player::Player>>) -> AppResult<String> {
    let health = player.opened().map(|engine| engine.audio_health()).unwrap_or_default();
    crate::commands::blocking("report_attachment", move || {
        let mut text = format!("System information\nrbxport {}\nOS: {} {}\nArchitecture: {}\nAudio deadline load: {:.1}%\nAudio callback overruns: {}\n",
            env!("CARGO_PKG_VERSION"), std::env::consts::OS,
            sysinfo::System::os_version().unwrap_or_default(), std::env::consts::ARCH,
            health.load * 100.0, health.xruns);
        let sample = crate::diagnostics::sample_shared();
        let _ = writeln!(text, "Process CPU: {:.1}% of one core\nResident memory: {:.1} MiB", sample.cpu, sample.memory_mb);
        text.push_str("\nApplication log (latest file, last 1 MiB)\n");
        text.push_str(&log_tail(&crate::logging::log_dir())?);
        Ok(text)
    }).await
}

fn log_tail(dir: &std::path::Path) -> AppResult<String> {
    let mut paths: Vec<_> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(Result::ok).map(|entry| entry.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rbxport")) && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("log"))).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("No application log available.\n".into()),
        Err(e) => return Err(AppError::internal(format!("The log directory could not be read: {e}"))),
    };
    paths.sort();
    let Some(path) = paths.last() else { return Ok("No application log available.\n".into()) };
    let mut file = std::fs::File::open(path).map_err(|e| AppError::internal(e.to_string()))?;
    let size = file.metadata().map_err(|e| AppError::internal(e.to_string()))?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(1_048_576))).map_err(|e| AppError::internal(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(1_048_576).read_to_end(&mut bytes).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The log is last, while system information precedes the user's report.
fn report_text(email: &str, description: &str, attachment: &str) -> AppResult<String> {
    if email.len() > 320 || description.trim().is_empty() || description.len() > 100_000 || attachment.len() > 2_000_000 {
        return Err(AppError::internal("The report is empty or too large."));
    }
    let (system, log) = attachment.split_once("\nApplication log").unwrap_or((attachment, ""));
    let date = time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let mut report = format!("{system}\nDate: {date}\nEmail: {}\n\nWhat happened\n{}\n", email.trim(), description.trim());
    if !log.is_empty() { report.push_str("\nApplication log"); report.push_str(log); }
    Ok(report)
}

fn zip_report(text: &str) -> AppResult<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("report.txt", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))
        .map_err(|e| AppError::internal(e.to_string()))?;
    zip.write_all(text.as_bytes()).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(zip.finish().map_err(|e| AppError::internal(e.to_string()))?.into_inner())
}

#[tauri::command]
pub async fn save_bug_report(path: String, email: String, description: String, attachment: String) -> AppResult<()> {
    crate::commands::blocking("save_bug_report", move || {
        let text = report_text(&email, &description, &attachment)?;
        let bytes = zip_report(&text)?;
        crate::grid::write_atomically(std::path::Path::new(&path), &bytes)
            .map_err(|e| AppError::internal(format!("The report could not be saved: {e}")))
    }).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn archive_contains_one_text_with_only_the_approved_attachment() {
        let text = report_text("dj@example.com", "The deck stopped", "System information\nOS: test\nApplication log\napproved log").unwrap();
        assert!(text.find("System information").unwrap() < text.find("Date:").unwrap());
        assert!(text.ends_with("approved log"));
        let bytes = zip_report(&text).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 1);
        let mut read = String::new();
        archive.by_name("report.txt").unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, text);
        let private = report_text("", "Problem", "").unwrap();
        assert!(!private.contains("System information"));
        assert!(!private.contains("Application log"));
    }
}
