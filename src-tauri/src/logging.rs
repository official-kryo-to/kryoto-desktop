//! The client's log, and the error reports that go to kryo.to.
//!
//! Everything notable goes to `logs/kryoto-desktop.log` in the app's data
//! folder - one line each, `time LEVEL scope message` - which Settings > Logs
//! shows and can open. Errors and crashes are also queued and sent to
//! kryo.to's `/api/desktop/reports` every minute (unless the player turned
//! that off), where staff see them grouped by version and message.
//!
//! A panic writes itself to the log and to `last-crash.txt` before the process
//! goes; the next start reports it, since a crashing process cannot be trusted
//! to finish a network request.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, Runtime};

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_QUEUED: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub at: u64,
    pub level: String,
    pub scope: String,
    pub message: String,
}

struct Logger {
    dir: PathBuf,
    queue: Mutex<Vec<Entry>>,
    account: Mutex<Option<String>>,
    file_lock: Mutex<()>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `2026-09-25 14:03:07Z` from unix seconds, without a date crate.
pub fn stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn log_path(dir: &std::path::Path) -> PathBuf {
    dir.join("kryoto-desktop.log")
}

/// Start logging. Call once, first thing in `setup`.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let dir = app
        .path()
        .app_data_dir()
        .map(|d| d.join("logs"))
        .unwrap_or_else(|_| std::env::temp_dir().join("kryoto-desktop-logs"));
    let _ = std::fs::create_dir_all(&dir);
    let _ = LOGGER.set(Logger {
        dir: dir.clone(),
        queue: Mutex::new(Vec::new()),
        account: Mutex::new(None),
        file_lock: Mutex::new(()),
    });

    // A crash from last time, reported now that there is a process to do it.
    let crash = dir.join("last-crash.txt");
    if let Ok(text) = std::fs::read_to_string(&crash) {
        queue(Entry { at: now(), level: "crash".into(), scope: "panic".into(), message: text });
        let _ = std::fs::remove_file(crash);
    }

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let text = format!("v{} {payload}{location}", env!("CARGO_PKG_VERSION"));
        write_line("crash", "panic", &text);
        if let Some(l) = LOGGER.get() {
            let _ = std::fs::write(l.dir.join("last-crash.txt"), &text);
        }
        previous(info);
    }));

    info("app", &format!("Kryoto Desktop {} started on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS));
}

fn write_line(level: &str, scope: &str, message: &str) {
    let Some(l) = LOGGER.get() else { return };
    let _guard = l.file_lock.lock();
    let path = log_path(&l.dir);
    if std::fs::metadata(&path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, l.dir.join("kryoto-desktop.1.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let one_line = message.replace('\n', " | ");
        let _ = writeln!(f, "{} {:<5} {scope}: {one_line}", stamp(now()), level.to_uppercase());
    }
}

fn queue(mut entry: Entry) {
    entry.message = redact_report(&entry.message).chars().take(4000).collect();
    if let Some(l) = LOGGER.get() {
        if let Ok(mut q) = l.queue.lock() {
            if q.len() < MAX_QUEUED {
                q.push(entry);
            }
        }
    }
}

/// Redact at the outbound queue boundary, including recovered panic records.
pub(crate) fn redact_report(message: &str) -> String {
    static URLS: OnceLock<regex::Regex> = OnceLock::new();
    static FIELDS: OnceLock<regex::Regex> = OnceLock::new();
    static BEARER: OnceLock<regex::Regex> = OnceLock::new();
    static COOKIES: OnceLock<regex::Regex> = OnceLock::new();
    let urls = URLS.get_or_init(|| regex::Regex::new(r#"(?i)https?://[^\s<>\"']+"#).expect("report URL regex"));
    let fields = FIELDS.get_or_init(|| regex::Regex::new(r#"(?i)\b(password|token|secret|api[_-]?key|authorization)"?\s*[:=]\s*(?:"[^"]*"|'[^']*'|[^\s,;]+)"#).expect("report field regex"));
    let bearer = BEARER.get_or_init(|| regex::Regex::new(r"(?i)\bbearer\s+[^\s,;]+").expect("report bearer regex"));
    let cookies = COOKIES.get_or_init(|| regex::Regex::new(r#"(?i)\bcookie"?\s*[:=][^\r\n]*"#).expect("report cookie regex"));
    let mut text = urls.replace_all(message, |captures: &regex::Captures<'_>| {
        let Ok(mut url) = url::Url::parse(&captures[0]) else { return "[redacted URL]".to_string() };
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        if url.path().starts_with("/d/") { url.set_path("/d/[redacted]"); }
        url.to_string()
    }).into_owned();
    text = bearer.replace_all(&text, "Bearer [redacted]").into_owned();
    text = fields.replace_all(&text, "$1=[redacted]").into_owned();
    text = cookies.replace_all(&text, "Cookie: [redacted]").into_owned();
    for name in ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA"] {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                text = text.replace(&value, "~").replace(&value.replace('\\', "/"), "~");
            }
        }
    }
    text
}

pub fn info(scope: &str, message: &str) {
    write_line("info", scope, message);
}

pub fn warn(scope: &str, message: &str) {
    write_line("warn", scope, message);
}

/// Written to the log, and queued for kryo.to.
pub fn error(scope: &str, message: &str) {
    write_line("error", scope, message);
    queue(Entry { at: now(), level: "error".into(), scope: scope.into(), message: message.into() });
}

/// Send what is queued. Keeps it on failure for the next attempt.
async fn flush<R: Runtime>(app: &AppHandle<R>) {
    let Some(l) = LOGGER.get() else { return };
    // Development builds do not report to kryo.to.
    let settings = crate::settings::load(app);
    if cfg!(debug_assertions) || !settings.send_reports {
        if let Ok(mut q) = l.queue.lock() {
            q.clear();
        }
        return;
    }
    let batch: Vec<Entry> = match l.queue.lock() {
        Ok(q) if !q.is_empty() => q.clone(),
        _ => return,
    };
    let body = serde_json::json!({
        "installId": install_id(app),
        "version": env!("CARGO_PKG_VERSION"),
        "os": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "account": l.account.lock().ok().and_then(|a| a.clone()),
        "entries": batch,
    });
    let Ok(client) = reqwest::Client::builder()
        .user_agent(concat!("KryotoDesktop/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build()
    else {
        return;
    };
    let endpoint = crate::settings::catalog_endpoint(&settings);
    if let Ok(res) = client.post(format!("{endpoint}/api/desktop/reports")).json(&body).send().await {
        if res.status().is_success() {
            if let Ok(mut q) = l.queue.lock() {
                let sent = batch.len().min(q.len());
                q.drain(..sent);
            }
        }
    }
}

/// Every minute, and once shortly after start (for last run's crash).
pub fn start_reporter<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        loop {
            flush(&app).await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// A random id for this install, so reports from one machine group together
/// without saying whose machine it is.
pub(crate) fn install_id<R: Runtime>(app: &AppHandle<R>) -> String {
    let Ok(dir) = app.path().app_data_dir() else { return "unknown".into() };
    let file = dir.join("install-id");
    if let Ok(id) = std::fs::read_to_string(&file) {
        if id.trim().len() >= 16 {
            return id.trim().to_string();
        }
    }
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
        ^ ((std::process::id() as u128) << 64);
    let id = format!("{:032x}", seed.wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835));
    let _ = std::fs::write(&file, &id);
    id
}

/* ── commands ─────────────────────────────────────────────── */

/// The window's own errors: `window.onerror`, rejected promises, and the
/// failures the shell catches and wants on record.
#[tauri::command]
pub fn log_write(level: String, scope: String, message: String) {
    let scope: String = scope.chars().take(60).collect();
    match level.as_str() {
        "error" => error(&scope, &message),
        "warn" => warn(&scope, &message),
        _ => info(&scope, &message),
    }
}

/// Who is signed in, for the reports (so a problem can be followed up).
/// Set from what kryo.to itself reports, never from the shell.
pub fn set_account(account: Option<String>) {
    if let Some(l) = LOGGER.get() {
        if let Ok(mut a) = l.account.lock() {
            *a = account;
        }
    }
}

/// The last `lines` lines of the log, for Settings > Logs.
#[tauri::command(async)]
pub fn logs_tail(lines: usize) -> String {
    let Some(l) = LOGGER.get() else { return String::new() };
    let text = std::fs::read_to_string(log_path(&l.dir)).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines.clamp(1, 2000))..].join("\n")
}

#[tauri::command(async)]
pub fn logs_folder() -> String {
    LOGGER.get().map(|l| l.dir.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Send the queue now (Settings > Logs > Send report).
#[tauri::command]
pub async fn logs_send(app: AppHandle) -> Result<usize, String> {
    let queued = LOGGER.get().and_then(|l| l.queue.lock().ok().map(|q| q.len())).unwrap_or(0);
    flush(&app).await;
    let left = LOGGER.get().and_then(|l| l.queue.lock().ok().map(|q| q.len())).unwrap_or(0);
    if left > 0 && crate::settings::load(&app).send_reports {
        return Err("The selected endpoint did not take the report. It stays queued and is tried again every minute.".into());
    }
    Ok(queued - left)
}

#[cfg(test)]
mod tests {
    #[test]
    fn outbound_reports_redact_credentials_and_signed_links() {
        let input = "GET https://user:pw@dl.kryo.to/d/signed-secret?token=query-secret#fragment Authorization: Bearer abc123 password=hunter2 api_key=secret456";
        let result = super::redact_report(input);
        for secret in ["user:pw", "signed-secret", "query-secret", "fragment", "abc123", "hunter2", "secret456"] {
            assert!(!result.contains(secret), "report retained {secret}: {result}");
        }
        assert!(result.contains("dl.kryo.to"));
        let result = super::redact_report(r#"{"password":"a secret with spaces", "token":"json-secret"}
Cookie: session=first-secret; refresh=second-secret"#);
        for secret in ["a secret", "json-secret", "first-secret", "second-secret"] { assert!(!result.contains(secret), "{result}"); }
    }
    #[test]
    fn stamps_read_as_dates() {
        assert_eq!(super::stamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(super::stamp(1_790_345_000), "2026-09-25 14:03:20Z");
    }
}
