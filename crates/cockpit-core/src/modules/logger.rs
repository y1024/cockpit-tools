use crate::modules::account::get_data_dir;
use chrono::{DateTime, Duration, Local};
use regex::{Captures, Regex};
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tracing::{error, info, warn};
use tracing_subscriber::{
    filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

const LOG_FILE_BASENAME: &str = "app";
const PLATFORM_LOG_FILE_PREFIX_ENV: &str = "COCKPIT_PLATFORM_LOG_FILE_PREFIX";
const CODEX_API_LOG_TARGET: &str = "codex_api";
const LOG_RETENTION_DAYS: i64 = 3;
const DEFAULT_LOG_TAIL_LINES: usize = 200;
const MIN_LOG_TAIL_LINES: usize = 20;
const MAX_LOG_TAIL_LINES: usize = 5000;
const LOG_TAIL_SCAN_CHUNK_BYTES: usize = 8192;
static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b")
        .expect("email regex should be valid")
});

struct LocalTimer;

impl tracing_subscriber::fmt::time::FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();
        write!(w, "{}", now.to_rfc3339())
    }
}

#[derive(Clone)]
struct DailyLogMakeWriter {
    log_dir: Arc<PathBuf>,
    basename: String,
}

struct DailyLogWriter {
    file: Option<File>,
}

impl DailyLogMakeWriter {
    fn new(log_dir: PathBuf, basename: impl Into<String>) -> Self {
        Self {
            log_dir: Arc::new(log_dir),
            basename: basename.into(),
        }
    }
}

impl DailyLogWriter {
    fn new(log_dir: &Path, basename: &str) -> Self {
        let file_name = current_daily_log_file_name(basename);
        let path = log_dir.join(file_name);
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Self { file }
    }
}

impl Write for DailyLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.file.as_mut() {
            Some(file) => file.write(buf),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for DailyLogMakeWriter {
    type Writer = DailyLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        DailyLogWriter::new(&self.log_dir, &self.basename)
    }
}

pub fn get_log_dir() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let log_dir = data_dir.join("logs");

    if !log_dir.exists() {
        fs::create_dir_all(&log_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
    }

    Ok(log_dir)
}

fn is_app_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(is_app_log_file_name)
        .unwrap_or(false)
}

fn is_app_log_file_name(name: &str) -> bool {
    if name == "app.log" {
        return true;
    }
    if let Some(date) = name.strip_prefix("app.log.") {
        return chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok();
    }
    name.strip_prefix("app-")
        .and_then(|rest| rest.strip_suffix(".log"))
        .map(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok())
        .unwrap_or(false)
}

fn current_daily_log_file_name(basename: &str) -> String {
    format!("{}-{}.log", basename, Local::now().format("%Y-%m-%d"))
}

fn resolve_log_file_prefix() -> (String, bool) {
    std::env::var(PLATFORM_LOG_FILE_PREFIX_ENV)
        .ok()
        .and_then(|value| sanitize_log_file_prefix(&value))
        .map(|value| (value, true))
        .unwrap_or_else(|| (LOG_FILE_BASENAME.to_string(), false))
}

fn sanitize_log_file_prefix(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trimmed = trimmed.strip_suffix(".log").unwrap_or(trimmed);
    if !trimmed.starts_with("platform-") {
        return None;
    }
    let safe = trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if !safe || trimmed.contains('/') || trimmed.contains('\\') {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn clamp_log_tail_lines(line_limit: Option<usize>) -> usize {
    line_limit
        .unwrap_or(DEFAULT_LOG_TAIL_LINES)
        .clamp(MIN_LOG_TAIL_LINES, MAX_LOG_TAIL_LINES)
}

pub fn get_latest_app_log_file() -> Result<PathBuf, String> {
    let log_dir = get_log_dir()?;
    let entries = fs::read_dir(&log_dir).map_err(|e| format!("读取日志目录失败: {}", e))?;

    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取日志目录项失败: {}", e))?;
        let path = entry.path();
        if !path.is_file() || !is_app_log_file(&path) {
            continue;
        }

        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &latest {
            Some((current_path, current_modified)) => {
                let should_replace = modified > *current_modified
                    || (modified == *current_modified
                        && path.file_name().and_then(|name| name.to_str())
                            > current_path.file_name().and_then(|name| name.to_str()));
                if should_replace {
                    latest = Some((path, modified));
                }
            }
            None => {
                latest = Some((path, modified));
            }
        }
    }

    latest
        .map(|(path, _)| path)
        .ok_or_else(|| "未找到可用日志文件".to_string())
}

pub fn read_log_tail_lines(log_file: &Path, line_limit: usize) -> Result<String, String> {
    let line_limit = line_limit.max(1);
    let mut file = File::open(log_file).map_err(|e| format!("打开日志文件失败: {}", e))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("读取日志文件元数据失败: {}", e))?
        .len();

    if file_len == 0 {
        return Ok(String::new());
    }

    let mut pos = file_len;
    let mut newline_count = 0usize;
    let mut start_offset = 0u64;
    let mut buffer = [0u8; LOG_TAIL_SCAN_CHUNK_BYTES];

    'scan: while pos > 0 {
        let read_size = usize::min(LOG_TAIL_SCAN_CHUNK_BYTES, pos as usize);
        pos -= read_size as u64;

        file.seek(SeekFrom::Start(pos))
            .map_err(|e| format!("读取日志定位失败: {}", e))?;
        file.read_exact(&mut buffer[..read_size])
            .map_err(|e| format!("读取日志内容失败: {}", e))?;

        for idx in (0..read_size).rev() {
            if buffer[idx] != b'\n' {
                continue;
            }
            newline_count += 1;
            if newline_count > line_limit {
                start_offset = pos + idx as u64 + 1;
                break 'scan;
            }
        }
    }

    file.seek(SeekFrom::Start(start_offset))
        .map_err(|e| format!("读取日志定位失败: {}", e))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("读取日志内容失败: {}", e))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn cleanup_expired_logs(log_dir: &Path) {
    let cutoff = Local::now() - Duration::days(LOG_RETENTION_DAYS);
    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(err) => {
            warn!("读取日志目录失败，跳过清理: {}", err);
            return;
        }
    };

    let mut removed_count = 0usize;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!("读取日志文件失败，已忽略: {}", err);
                continue;
            }
        };

        let path = entry.path();
        if !path.is_file() || !is_app_log_file(&path) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                warn!("读取日志元数据失败，已忽略: {:?}, {}", path, err);
                continue;
            }
        };

        let modified_at = match metadata.modified() {
            Ok(time) => {
                let dt: DateTime<Local> = time.into();
                dt
            }
            Err(err) => {
                warn!("读取日志修改时间失败，已忽略: {:?}, {}", path, err);
                continue;
            }
        };

        if modified_at >= cutoff {
            continue;
        }

        match fs::remove_file(&path) {
            Ok(_) => removed_count += 1,
            Err(err) => warn!("删除过期日志失败，已忽略: {:?}, {}", path, err),
        }
    }

    if removed_count > 0 {
        info!(
            "日志清理完成：删除 {} 个超过 {} 天的日志文件",
            removed_count, LOG_RETENTION_DAYS
        );
    }
}

/// 初始化日志系统
pub fn init_logger() {
    let _ = tracing_log::LogTracer::init();

    let log_dir = match get_log_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("无法初始化日志目录: {}", e);
            return;
        }
    };

    let (log_file_prefix, is_platform_logger) = resolve_log_file_prefix();
    let file_writer = DailyLogMakeWriter::new(log_dir.clone(), log_file_prefix);

    let console_layer = fmt::Layer::new()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .with_timer(LocalTimer)
        .with_filter(filter_fn(move |_| !is_platform_logger));

    let file_layer = fmt::Layer::new()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .with_timer(LocalTimer);

    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(console_layer)
        .with(file_layer)
        .try_init();

    info!("日志系统已完成初始化");

    // 日志清理移至后台线程，不阻塞启动
    std::thread::spawn(move || {
        cleanup_expired_logs(&log_dir);
    });
}

pub fn log_info(message: &str) {
    info!("{}", sanitize_message(message));
}

pub fn log_warn(message: &str) {
    warn!("{}", sanitize_message(message));
}

pub fn log_error(message: &str) {
    error!("{}", sanitize_message(message));
}

pub fn log_codex_api_info(message: &str) {
    info!(target: CODEX_API_LOG_TARGET, "{}", sanitize_message(message));
}

pub fn log_codex_api_warn(message: &str) {
    warn!(target: CODEX_API_LOG_TARGET, "{}", sanitize_message(message));
}

pub fn log_codex_api_error(message: &str) {
    error!(target: CODEX_API_LOG_TARGET, "{}", sanitize_message(message));
}

fn sanitize_message(message: &str) -> String {
    EMAIL_REGEX
        .replace_all(message, |caps: &Captures| mask_email(&caps[0]))
        .to_string()
}

fn mask_email(email: &str) -> String {
    let (local, domain) = match email.split_once('@') {
        Some(parts) => parts,
        None => return email.to_string(),
    };

    format!("{}@{}", mask_local_part(local), mask_domain_part(domain))
}

fn mask_local_part(local: &str) -> String {
    let chars: Vec<char> = local.chars().collect();
    match chars.len() {
        0 => "***".to_string(),
        1 => "*".to_string(),
        2 => format!("{}*", chars[0]),
        3 => format!("{}*{}", chars[0], chars[2]),
        _ => format!("{}{}***{}", chars[0], chars[1], chars[chars.len() - 1]),
    }
}

fn mask_domain_part(domain: &str) -> String {
    let mut parts = domain.split('.');
    let head = parts.next().unwrap_or_default();
    let tail = parts.collect::<Vec<&str>>();

    let masked_head = mask_domain_head(head);
    if tail.is_empty() {
        masked_head
    } else {
        format!("{}.{}", masked_head, tail.join("."))
    }
}

fn mask_domain_head(head: &str) -> String {
    let chars: Vec<char> = head.chars().collect();
    match chars.len() {
        0 => "***".to_string(),
        1 => "*".to_string(),
        2 => format!("{}*", chars[0]),
        _ => format!("{}***{}", chars[0], chars[chars.len() - 1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_platform_log_prefix_env() {
        assert_eq!(
            sanitize_log_file_prefix("platform-zed"),
            Some("platform-zed".to_string())
        );
        assert_eq!(
            sanitize_log_file_prefix("platform-github-copilot.log"),
            Some("platform-github-copilot".to_string())
        );
        assert_eq!(
            sanitize_log_file_prefix(" platform-codebuddy_cn "),
            Some("platform-codebuddy_cn".to_string())
        );

        assert_eq!(sanitize_log_file_prefix("app"), None);
        assert_eq!(sanitize_log_file_prefix("platform-../zed"), None);
        assert_eq!(
            sanitize_log_file_prefix("platform-zed.log.2026-06-24"),
            None
        );
    }

    #[test]
    fn recognizes_new_and_legacy_app_logs() {
        assert!(is_app_log_file(Path::new("app-2026-06-24.log")));
        assert!(is_app_log_file(Path::new("app.log.2026-06-24")));
        assert!(is_app_log_file(Path::new("app.log")));

        assert!(!is_app_log_file(Path::new("app-2026-06-24.txt")));
        assert!(!is_app_log_file(Path::new("app.login")));
        assert!(!is_app_log_file(Path::new("platform-zed-2026-06-24.log")));
    }
}
