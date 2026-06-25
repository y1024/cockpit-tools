use crate::modules::app_data::get_data_dir;
use chrono::{DateTime, Duration, Local, NaiveDate};
use regex::{Captures, Regex};
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, OnceLock};
use tracing::{error, info, warn};
use tracing_subscriber::{
    filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

const APP_LOG_BASENAME: &str = "app";
const CODEX_API_LOG_BASENAME: &str = "codex-api";
const LEGACY_APP_LOG_FILE_PREFIX: &str = "app.log";
const LEGACY_CODEX_API_LOG_FILE_PREFIX: &str = "codex-api.log";
const CODEX_API_LOG_TARGET: &str = "codex_api";
const PLATFORM_LOG_FILE_PREFIX: &str = "platform-";
const LOG_FILE_SUFFIX: &str = ".log";
const LEGACY_PLATFORM_LOG_FILE_MARKER: &str = ".log";
const LOG_RETENTION_DAYS: i64 = 3;
const DEFAULT_LOG_TAIL_LINES: usize = 200;
const MIN_LOG_TAIL_LINES: usize = 20;
const MAX_LOG_TAIL_LINES: usize = 5000;
const LOG_TAIL_SCAN_CHUNK_BYTES: usize = 8192;
static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b")
        .expect("email regex should be valid")
});
static DUPLICATED_PLATFORM_DAILY_LOG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^platform-(?P<platform>[A-Za-z0-9_-]+?)-(?P<date>\d{4}-\d{2}-\d{2})(?:-\d{4}-\d{2}-\d{2})+\.log$",
    )
    .expect("duplicated platform daily log regex should be valid")
});
static LOG_FILE_NAME_MIGRATION_DONE: OnceLock<()> = OnceLock::new();

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

    migrate_legacy_log_file_names_once(&log_dir);

    Ok(log_dir)
}

fn current_daily_log_file_name(basename: &str) -> String {
    format!("{}-{}.log", basename, Local::now().format("%Y-%m-%d"))
}

fn is_valid_log_date(value: &str) -> bool {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn new_daily_log_date_for_basename<'a>(name: &'a str, basename: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(basename)?.strip_prefix('-')?;
    let date = rest.strip_suffix(LOG_FILE_SUFFIX)?;
    if is_valid_log_date(date) {
        Some(date)
    } else {
        None
    }
}

fn is_new_daily_log_file_name(name: &str, basename: &str) -> bool {
    new_daily_log_date_for_basename(name, basename).is_some()
}

fn legacy_log_date_for_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    if name == prefix {
        return None;
    }
    let date = name.strip_prefix(prefix)?.strip_prefix('.')?;
    if is_valid_log_date(date) {
        Some(date)
    } else {
        None
    }
}

fn is_legacy_log_file_with_prefix(name: &str, prefix: &str) -> bool {
    name == prefix || legacy_log_date_for_prefix(name, prefix).is_some()
}

fn is_app_log_file_name(name: &str) -> bool {
    is_new_daily_log_file_name(name, APP_LOG_BASENAME)
        || is_legacy_log_file_with_prefix(name, LEGACY_APP_LOG_FILE_PREFIX)
}

fn is_codex_api_log_file_name(name: &str) -> bool {
    is_new_daily_log_file_name(name, CODEX_API_LOG_BASENAME)
        || is_legacy_log_file_with_prefix(name, LEGACY_CODEX_API_LOG_FILE_PREFIX)
}

fn is_managed_log_file_name(name: &str) -> bool {
    is_app_log_file_name(name)
        || is_codex_api_log_file_name(name)
        || is_platform_log_file_name(name)
}

fn is_platform_log_file_name(name: &str) -> bool {
    parse_new_platform_log_file_name(name).is_some()
        || parse_legacy_platform_log_file_name(name).is_some()
}

fn parse_new_platform_log_file_name(name: &str) -> Option<(&str, &str)> {
    let stem = name.strip_suffix(LOG_FILE_SUFFIX)?;
    if !stem.starts_with(PLATFORM_LOG_FILE_PREFIX) {
        return None;
    }
    if stem.len() < PLATFORM_LOG_FILE_PREFIX.len() + 1 + 1 + 10 {
        return None;
    }
    let date_start = stem.len().checked_sub(10)?;
    if stem.as_bytes().get(date_start.checked_sub(1)?) != Some(&b'-') {
        return None;
    }
    let date = &stem[date_start..];
    if !is_valid_log_date(date) {
        return None;
    }
    let platform_stem = &stem[..date_start - 1];
    let platform_id = platform_stem.strip_prefix(PLATFORM_LOG_FILE_PREFIX)?;
    if platform_id.is_empty()
        || !platform_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    Some((platform_id, date))
}

fn parse_legacy_platform_log_file_name(name: &str) -> Option<(&str, Option<&str>)> {
    if parse_new_platform_log_file_name(name).is_some() {
        return None;
    }

    let rest = name.strip_prefix(PLATFORM_LOG_FILE_PREFIX)?;
    let (platform_id, suffix) = rest.split_once(LEGACY_PLATFORM_LOG_FILE_MARKER)?;
    if platform_id.is_empty()
        || !platform_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    if suffix.is_empty() {
        return Some((platform_id, None));
    }
    let date = suffix.strip_prefix('.')?;
    if !is_valid_log_date(date) {
        return None;
    }
    Some((platform_id, Some(date)))
}

fn duplicated_platform_daily_log_target_file_name(name: &str) -> Option<String> {
    let captures = DUPLICATED_PLATFORM_DAILY_LOG_REGEX.captures(name)?;
    let platform_id = captures.name("platform")?.as_str();
    let date = captures.name("date")?.as_str();
    if platform_id.is_empty()
        || !platform_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        || !is_valid_log_date(date)
    {
        return None;
    }

    Some(format!("platform-{}-{}.log", platform_id, date))
}

fn legacy_log_display_name(name: &str, legacy_prefix: &str, basename: &str) -> Option<String> {
    legacy_log_date_for_prefix(name, legacy_prefix).map(|date| format!("{}-{}.log", basename, date))
}

fn local_log_date_from_modified_time(modified_at: Option<std::time::SystemTime>) -> String {
    let time = modified_at.unwrap_or_else(std::time::SystemTime::now);
    let date_time: DateTime<Local> = time.into();
    date_time.format("%Y-%m-%d").to_string()
}

fn legacy_managed_log_target_file_name(
    name: &str,
    modified_at: Option<std::time::SystemTime>,
) -> Option<String> {
    if let Some(target_file_name) = duplicated_platform_daily_log_target_file_name(name) {
        return Some(target_file_name);
    }

    if let Some(date) = legacy_log_date_for_prefix(name, LEGACY_APP_LOG_FILE_PREFIX) {
        return Some(format!("{}-{}.log", APP_LOG_BASENAME, date));
    }
    if name == LEGACY_APP_LOG_FILE_PREFIX {
        return Some(format!(
            "{}-{}.log",
            APP_LOG_BASENAME,
            local_log_date_from_modified_time(modified_at)
        ));
    }

    if let Some(date) = legacy_log_date_for_prefix(name, LEGACY_CODEX_API_LOG_FILE_PREFIX) {
        return Some(format!("{}-{}.log", CODEX_API_LOG_BASENAME, date));
    }
    if name == LEGACY_CODEX_API_LOG_FILE_PREFIX {
        return Some(format!(
            "{}-{}.log",
            CODEX_API_LOG_BASENAME,
            local_log_date_from_modified_time(modified_at)
        ));
    }

    if let Some((platform_id, date)) = parse_legacy_platform_log_file_name(name) {
        let date = date
            .map(ToString::to_string)
            .unwrap_or_else(|| local_log_date_from_modified_time(modified_at));
        return Some(format!("platform-{}-{}.log", platform_id, date));
    }

    None
}

pub fn display_log_file_name(file_name: &str) -> String {
    if let Some(display_name) =
        legacy_log_display_name(file_name, LEGACY_APP_LOG_FILE_PREFIX, APP_LOG_BASENAME)
    {
        return display_name;
    }
    if let Some(display_name) = legacy_log_display_name(
        file_name,
        LEGACY_CODEX_API_LOG_FILE_PREFIX,
        CODEX_API_LOG_BASENAME,
    ) {
        return display_name;
    }
    if let Some((platform_id, Some(date))) = parse_legacy_platform_log_file_name(file_name) {
        return format!("platform-{}-{}.log", platform_id, date);
    }
    file_name.to_string()
}

fn migrate_legacy_log_file_names_once(log_dir: &Path) {
    LOG_FILE_NAME_MIGRATION_DONE.get_or_init(|| {
        if let Err(err) = migrate_legacy_log_file_names(log_dir) {
            eprintln!("迁移历史日志文件名失败: {}", err);
        }
    });
}

fn migrate_legacy_log_file_names(log_dir: &Path) -> Result<usize, String> {
    let entries = fs::read_dir(log_dir).map_err(|e| format!("读取日志目录失败: {}", e))?;
    let mut migrated_count = 0usize;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("读取历史日志目录项失败，已跳过: {}", err);
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let modified_at = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        let Some(target_file_name) = legacy_managed_log_target_file_name(file_name, modified_at)
        else {
            continue;
        };
        if target_file_name == file_name {
            continue;
        }

        let target_path = log_dir.join(target_file_name);
        match migrate_legacy_log_file(&path, &target_path) {
            Ok(true) => migrated_count += 1,
            Ok(false) => {}
            Err(err) => eprintln!("迁移历史日志文件失败，已跳过: {:?}, {}", path, err),
        }
    }

    Ok(migrated_count)
}

fn migrate_legacy_log_file(path: &Path, target_path: &Path) -> Result<bool, String> {
    if path == target_path {
        return Ok(false);
    }

    if !target_path.exists() {
        fs::rename(path, target_path).map_err(|e| {
            format!(
                "重命名历史日志失败: {} -> {}, {}",
                path.display(),
                target_path.display(),
                e
            )
        })?;
        return Ok(true);
    }

    merge_legacy_log_file(path, target_path)?;
    fs::remove_file(path)
        .map_err(|e| format!("删除已合并的历史日志失败: {}, {}", path.display(), e))?;
    Ok(true)
}

fn merge_legacy_log_file(path: &Path, target_path: &Path) -> Result<(), String> {
    let legacy_content =
        fs::read(path).map_err(|e| format!("读取历史日志失败: {}, {}", path.display(), e))?;
    let target_content = fs::read(target_path)
        .map_err(|e| format!("读取目标日志失败: {}, {}", target_path.display(), e))?;
    let target_file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("log");
    let tmp_path = target_path.with_file_name(format!(".{}.migration", target_file_name));
    if tmp_path.exists() {
        fs::remove_file(&tmp_path)
            .map_err(|e| format!("清理日志迁移临时文件失败: {}, {}", tmp_path.display(), e))?;
    }

    let mut tmp_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp_path)
        .map_err(|e| format!("创建日志迁移临时文件失败: {}, {}", tmp_path.display(), e))?;
    tmp_file
        .write_all(&legacy_content)
        .map_err(|e| format!("写入历史日志内容失败: {}, {}", tmp_path.display(), e))?;
    if !legacy_content.is_empty() && !legacy_content.ends_with(b"\n") && !target_content.is_empty()
    {
        tmp_file
            .write_all(b"\n")
            .map_err(|e| format!("写入日志分隔换行失败: {}, {}", tmp_path.display(), e))?;
    }
    tmp_file
        .write_all(&target_content)
        .map_err(|e| format!("写入目标日志内容失败: {}, {}", tmp_path.display(), e))?;
    tmp_file
        .flush()
        .map_err(|e| format!("刷新日志迁移临时文件失败: {}, {}", tmp_path.display(), e))?;
    drop(tmp_file);

    fs::remove_file(target_path)
        .map_err(|e| format!("替换目标日志前删除失败: {}, {}", target_path.display(), e))?;
    fs::rename(&tmp_path, target_path).map_err(|e| {
        format!(
            "替换目标日志失败: {} -> {}, {}",
            tmp_path.display(),
            target_path.display(),
            e
        )
    })?;

    Ok(())
}

fn platform_id_from_log_message(message: &str) -> Option<&'static str> {
    const PREFIXES: &[(&str, &str)] = &[
        ("[Antigravity IDE", "antigravity_ide"),
        ("[Antigravity", "antigravity"),
        ("[Claude", "claude_manager"),
        ("[Claude Command]", "claude_manager"),
        ("[CodeBuddy CN", "codebuddy_cn"),
        ("[CodeBuddy", "codebuddy"),
        ("[CodeBuddy Command]", "codebuddy"),
        ("[Codex", "codex"),
        ("[AutoSwitch][Codex]", "codex"),
        ("[QuotaAlert][Codex]", "codex"),
        ("[Cursor", "cursor"),
        ("[Cursor Command]", "cursor"),
        ("[Gemini", "gemini"),
        ("[Gemini Command]", "gemini"),
        ("[GitHubCopilot Command]", "github-copilot"),
        ("[Kiro Command]", "kiro"),
        ("[Qoder", "qoder"),
        ("[Qoder Command]", "qoder"),
        ("[TokenKeeper][Kiro]", "kiro"),
        ("[TokenKeeper][WorkBuddy]", "workbuddy"),
        ("[Trae", "trae"),
        ("[Trae Command]", "trae"),
        ("[Windsurf", "windsurf"),
        ("[Windsurf Command]", "windsurf"),
        ("[WorkBuddy", "workbuddy"),
        ("[Zed", "zed"),
    ];
    if let Some(platform_id) = message
        .strip_prefix("[PlatformAdapter][")
        .and_then(|rest| rest.split_once(']').map(|(platform_id, _)| platform_id))
        .filter(|platform_id| !platform_id.is_empty())
    {
        return match platform_id {
            "antigravity" => Some("antigravity"),
            "antigravity_ide" => Some("antigravity_ide"),
            "claude_manager" => Some("claude_manager"),
            "codebuddy" => Some("codebuddy"),
            "codebuddy_cn" => Some("codebuddy_cn"),
            "codex" => Some("codex"),
            "cursor" => Some("cursor"),
            "gemini" => Some("gemini"),
            "github-copilot" => Some("github-copilot"),
            "kiro" => Some("kiro"),
            "qoder" => Some("qoder"),
            "trae" => Some("trae"),
            "windsurf" => Some("windsurf"),
            "workbuddy" => Some("workbuddy"),
            "zed" => Some("zed"),
            _ => None,
        };
    };
    PREFIXES
        .iter()
        .find_map(|(prefix, platform_id)| message.starts_with(prefix).then_some(*platform_id))
}

fn is_managed_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(is_managed_log_file_name)
        .unwrap_or(false)
}

pub fn clamp_log_tail_lines(line_limit: Option<usize>) -> usize {
    line_limit
        .unwrap_or(DEFAULT_LOG_TAIL_LINES)
        .clamp(MIN_LOG_TAIL_LINES, MAX_LOG_TAIL_LINES)
}

fn list_log_files_by_name<F>(matcher: F) -> Result<Vec<PathBuf>, String>
where
    F: Fn(&str) -> bool,
{
    let log_dir = get_log_dir()?;
    let entries = fs::read_dir(&log_dir).map_err(|e| format!("读取日志目录失败: {}", e))?;

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取日志目录项失败: {}", e))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !path.is_file() || !matcher(name) {
            continue;
        }
        paths.push(path);
    }

    paths.sort_by(|left, right| compare_log_paths_by_recency(left, right));
    Ok(paths)
}

fn compare_log_paths_by_recency(left: &PathBuf, right: &PathBuf) -> std::cmp::Ordering {
    let left_modified = fs::metadata(left)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let right_modified = fs::metadata(right)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    right_modified
        .cmp(&left_modified)
        .then_with(|| right.file_name().cmp(&left.file_name()))
}

pub fn list_managed_log_files() -> Result<Vec<PathBuf>, String> {
    list_log_files_by_name(is_managed_log_file_name)
}

pub fn resolve_managed_log_file(file_name: Option<&str>) -> Result<PathBuf, String> {
    let log_files = list_managed_log_files()?;
    if log_files.is_empty() {
        return Err("未找到可用日志文件".to_string());
    }

    if let Some(file_name) = file_name.map(str::trim).filter(|name| !name.is_empty()) {
        return log_files
            .into_iter()
            .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(file_name))
            .ok_or_else(|| format!("未找到指定日志文件: {}", file_name));
    }

    log_files
        .into_iter()
        .next()
        .ok_or_else(|| "未找到可用日志文件".to_string())
}

pub fn get_latest_app_log_file() -> Result<PathBuf, String> {
    list_log_files_by_name(is_app_log_file_name)?
        .into_iter()
        .next()
        .ok_or_else(|| "未找到可用日志文件".to_string())
}

pub fn platform_log_file_prefix(platform_id: &str) -> String {
    let sanitized = sanitize_platform_log_id(platform_id);
    format!("{}{}", PLATFORM_LOG_FILE_PREFIX, sanitized)
}

fn sanitize_platform_log_id(platform_id: &str) -> String {
    let sanitized = platform_id
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
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
        if !path.is_file() || !is_managed_log_file(&path) {
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

    let app_file_writer = DailyLogMakeWriter::new(log_dir.clone(), APP_LOG_BASENAME);
    let codex_api_file_writer = DailyLogMakeWriter::new(log_dir.clone(), CODEX_API_LOG_BASENAME);

    let console_layer = fmt::Layer::new()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .with_timer(LocalTimer);

    let app_file_layer = fmt::Layer::new()
        .with_writer(app_file_writer)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .with_timer(LocalTimer);

    let codex_api_file_layer = fmt::Layer::new()
        .with_writer(codex_api_file_writer)
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .with_timer(LocalTimer);

    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(console_layer)
        .with(app_file_layer.with_filter(filter_fn(|metadata| {
            metadata.target() != CODEX_API_LOG_TARGET
        })))
        .with(codex_api_file_layer.with_filter(filter_fn(|metadata| {
            metadata.target() == CODEX_API_LOG_TARGET
        })))
        .try_init();

    info!("日志系统已完成初始化");

    // 日志清理移至后台线程，不阻塞启动
    std::thread::spawn(move || {
        cleanup_expired_logs(&log_dir);
    });
}

pub fn log_info(message: &str) {
    log_with_platform_route("INFO", message, |sanitized| {
        info!("{}", sanitized);
    });
}

pub fn log_warn(message: &str) {
    log_with_platform_route("WARN", message, |sanitized| {
        warn!("{}", sanitized);
    });
}

pub fn log_error(message: &str) {
    log_with_platform_route("ERROR", message, |sanitized| {
        error!("{}", sanitized);
    });
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

fn log_with_platform_route<F>(level: &str, message: &str, fallback: F)
where
    F: FnOnce(&str),
{
    let sanitized = sanitize_message(message);
    if let Some(platform_id) = platform_id_from_log_message(&sanitized) {
        write_platform_log_line(platform_id, level, &sanitized);
        return;
    }
    fallback(&sanitized);
}

fn write_platform_log_line(platform_id: &str, level: &str, message: &str) {
    let Ok(log_dir) = get_log_dir() else {
        return;
    };
    let file_name = current_daily_log_file_name(&platform_log_file_prefix(platform_id));
    let path = log_dir.join(file_name);
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{}  {} {}", Local::now().to_rfc3339(), level, message);
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

    fn unique_test_log_dir(name: &str) -> PathBuf {
        let nanos = Local::now().timestamp_nanos_opt().unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "cockpit-logger-test-{}-{}-{}",
            name,
            std::process::id(),
            nanos
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("test log dir should be created");
        dir
    }

    #[test]
    fn recognizes_new_and_legacy_log_file_names() {
        assert!(is_managed_log_file_name("app-2026-06-24.log"));
        assert!(is_managed_log_file_name("codex-api-2026-06-24.log"));
        assert!(is_managed_log_file_name("platform-zed-2026-06-24.log"));
        assert!(is_managed_log_file_name(
            "platform-github-copilot-2026-06-24.log"
        ));

        assert!(is_managed_log_file_name("app.log.2026-06-24"));
        assert!(is_managed_log_file_name("codex-api.log.2026-06-24"));
        assert!(is_managed_log_file_name("platform-zed.log.2026-06-24"));

        assert!(!is_managed_log_file_name("app-2026-06-24.txt"));
        assert!(!is_managed_log_file_name("platform-zed-2026-06-24.txt"));
        assert!(!is_managed_log_file_name("platform-zed.log.bad-date"));
    }

    #[test]
    fn displays_legacy_log_files_with_new_aliases() {
        assert_eq!(
            display_log_file_name("app.log.2026-06-24"),
            "app-2026-06-24.log"
        );
        assert_eq!(
            display_log_file_name("codex-api.log.2026-06-24"),
            "codex-api-2026-06-24.log"
        );
        assert_eq!(
            display_log_file_name("platform-zed.log.2026-06-24"),
            "platform-zed-2026-06-24.log"
        );
        assert_eq!(
            display_log_file_name("platform-github-copilot.log.2026-06-24"),
            "platform-github-copilot-2026-06-24.log"
        );
    }

    #[test]
    fn does_not_treat_new_platform_logs_as_legacy_logs() {
        assert_eq!(
            parse_new_platform_log_file_name("platform-zed-2026-06-24.log"),
            Some(("zed", "2026-06-24"))
        );
        assert_eq!(
            parse_new_platform_log_file_name("platform-github-copilot-2026-06-24.log"),
            Some(("github-copilot", "2026-06-24"))
        );
        assert_eq!(
            parse_legacy_platform_log_file_name("platform-zed-2026-06-24.log"),
            None
        );
        assert_eq!(
            legacy_managed_log_target_file_name("platform-zed-2026-06-24.log", None),
            None
        );
    }

    #[test]
    fn maps_undated_legacy_logs_to_modified_date_names() {
        let modified_at = Some(std::time::UNIX_EPOCH);
        let date = local_log_date_from_modified_time(modified_at);

        assert_eq!(
            legacy_managed_log_target_file_name("app.log", modified_at),
            Some(format!("app-{}.log", date))
        );
        assert_eq!(
            legacy_managed_log_target_file_name("codex-api.log", modified_at),
            Some(format!("codex-api-{}.log", date))
        );
        assert_eq!(
            legacy_managed_log_target_file_name("platform-zed.log", modified_at),
            Some(format!("platform-zed-{}.log", date))
        );
    }

    #[test]
    fn migrates_legacy_log_files_to_new_suffix_names() {
        let dir = unique_test_log_dir("rename");
        fs::write(dir.join("app.log.2026-06-24"), "app legacy\n").unwrap();
        fs::write(dir.join("codex-api.log.2026-06-24"), "api legacy\n").unwrap();
        fs::write(dir.join("platform-zed.log.2026-06-24"), "zed legacy\n").unwrap();

        let migrated_count = migrate_legacy_log_file_names(&dir).unwrap();

        assert_eq!(migrated_count, 3);
        assert!(!dir.join("app.log.2026-06-24").exists());
        assert!(!dir.join("codex-api.log.2026-06-24").exists());
        assert!(!dir.join("platform-zed.log.2026-06-24").exists());
        assert_eq!(
            fs::read_to_string(dir.join("app-2026-06-24.log")).unwrap(),
            "app legacy\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("codex-api-2026-06-24.log")).unwrap(),
            "api legacy\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("platform-zed-2026-06-24.log")).unwrap(),
            "zed legacy\n"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn collapses_platform_logs_with_repeated_dates() {
        let dir = unique_test_log_dir("repeated-dates");
        fs::write(
            dir.join("platform-zed-2026-06-24-2026-06-25.log"),
            "zed repeated\n",
        )
        .unwrap();
        fs::write(
            dir.join("platform-github-copilot-2026-06-24-2026-06-25-2026-06-25.log"),
            "github repeated\n",
        )
        .unwrap();

        let migrated_count = migrate_legacy_log_file_names(&dir).unwrap();

        assert_eq!(migrated_count, 2);
        assert!(!dir.join("platform-zed-2026-06-24-2026-06-25.log").exists());
        assert!(!dir
            .join("platform-github-copilot-2026-06-24-2026-06-25-2026-06-25.log")
            .exists());
        assert_eq!(
            fs::read_to_string(dir.join("platform-zed-2026-06-24.log")).unwrap(),
            "zed repeated\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("platform-github-copilot-2026-06-24.log")).unwrap(),
            "github repeated\n"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn merges_repeated_date_platform_logs_into_existing_daily_log() {
        let dir = unique_test_log_dir("repeated-date-merge");
        fs::write(dir.join("platform-kiro-2026-06-25.log"), "new line\n").unwrap();
        fs::write(
            dir.join("platform-kiro-2026-06-25-2026-06-25.log"),
            "repeated line",
        )
        .unwrap();

        let migrated_count = migrate_legacy_log_file_names(&dir).unwrap();

        assert_eq!(migrated_count, 1);
        assert!(!dir.join("platform-kiro-2026-06-25-2026-06-25.log").exists());
        assert_eq!(
            fs::read_to_string(dir.join("platform-kiro-2026-06-25.log")).unwrap(),
            "repeated line\nnew line\n"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn merges_legacy_log_file_when_new_name_already_exists() {
        let dir = unique_test_log_dir("merge");
        fs::write(dir.join("app.log.2026-06-24"), "legacy line").unwrap();
        fs::write(dir.join("app-2026-06-24.log"), "new line\n").unwrap();

        let migrated_count = migrate_legacy_log_file_names(&dir).unwrap();

        assert_eq!(migrated_count, 1);
        assert!(!dir.join("app.log.2026-06-24").exists());
        assert_eq!(
            fs::read_to_string(dir.join("app-2026-06-24.log")).unwrap(),
            "legacy line\nnew line\n"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn routes_host_platform_facade_messages() {
        assert_eq!(
            platform_id_from_log_message("[Zed Command] 手动刷新账号开始"),
            Some("zed")
        );
        assert_eq!(
            platform_id_from_log_message("[Kiro Command] 手动批量刷新开始"),
            Some("kiro")
        );
        assert_eq!(
            platform_id_from_log_message("[CodeBuddy CN Command] 手动刷新账号开始"),
            Some("codebuddy_cn")
        );
        assert_eq!(
            platform_id_from_log_message("[GitHubCopilot Command] 手动刷新账号开始"),
            Some("github-copilot")
        );
        assert_eq!(
            platform_id_from_log_message("[Gemini Switch] 未检测到可用 macOS keychain"),
            Some("gemini")
        );
        assert_eq!(
            platform_id_from_log_message("[Windsurf PasswordLogin] Firebase 登录成功"),
            Some("windsurf")
        );
        assert_eq!(
            platform_id_from_log_message("[Qoder OAuth] 开始创建登录会话"),
            Some("qoder")
        );
        assert_eq!(
            platform_id_from_log_message("[Claude] closing Claude before profile write"),
            Some("claude_manager")
        );
        assert_eq!(
            platform_id_from_log_message("[PlatformAdapter][github-copilot][stderr] adapter log"),
            Some("github-copilot")
        );
        assert_eq!(platform_id_from_log_message("普通主应用日志"), None);
    }
}
