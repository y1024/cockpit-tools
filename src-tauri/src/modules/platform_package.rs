use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use url::Url;

use crate::modules::{atomic_write, logger};

const PLATFORM_PACKAGE_REGISTRY_FILE: &str = "platform_packages.json";
const PLATFORM_PACKAGE_INDEX_CACHE_FILE: &str = "platform_package_index_cache.json";
const PLATFORM_PACKAGE_INDEX_LOCAL_OVERRIDE_FILE: &str = "platform-package-index.local.json";
const PLATFORM_PACKAGE_DIR: &str = "platform-packages";
const MANIFEST_FILE: &str = "manifest.json";
const CURRENT_DIR: &str = "current";
const DOWNLOADS_DIR: &str = "downloads";
const ANTIGRAVITY_PLATFORM_ID: &str = "antigravity";
const ANTIGRAVITY_IDE_PLATFORM_ID: &str = "antigravity_ide";
const ZED_PLATFORM_ID: &str = "zed";
const CLAUDE_MANAGER_PLATFORM_ID: &str = "claude_manager";
const KIRO_PLATFORM_ID: &str = "kiro";
const GITHUB_COPILOT_PLATFORM_ID: &str = "github-copilot";
const WINDSURF_PLATFORM_ID: &str = "windsurf";
const CURSOR_PLATFORM_ID: &str = "cursor";
const GEMINI_PLATFORM_ID: &str = "gemini";
const TRAE_PLATFORM_ID: &str = "trae";
const QODER_PLATFORM_ID: &str = "qoder";
const CODEBUDDY_PLATFORM_ID: &str = "codebuddy";
const CODEBUDDY_CN_PLATFORM_ID: &str = "codebuddy_cn";
const WORKBUDDY_PLATFORM_ID: &str = "workbuddy";
const CODEX_PLATFORM_ID: &str = "codex";
const PLATFORM_PACKAGE_API_VERSION: u32 = 1;
const SUPPORTED_PLATFORM_IDS: &[&str] = &[
    ANTIGRAVITY_PLATFORM_ID,
    ANTIGRAVITY_IDE_PLATFORM_ID,
    CLAUDE_MANAGER_PLATFORM_ID,
    ZED_PLATFORM_ID,
    KIRO_PLATFORM_ID,
    GITHUB_COPILOT_PLATFORM_ID,
    WINDSURF_PLATFORM_ID,
    CURSOR_PLATFORM_ID,
    GEMINI_PLATFORM_ID,
    TRAE_PLATFORM_ID,
    QODER_PLATFORM_ID,
    CODEBUDDY_PLATFORM_ID,
    CODEBUDDY_CN_PLATFORM_ID,
    WORKBUDDY_PLATFORM_ID,
    CODEX_PLATFORM_ID,
];
const PLATFORM_PACKAGE_INDEX_URL: &str =
    "https://raw.githubusercontent.com/jlcodes99/cockpit-tools/main/platform-packages/index.json";
const PLATFORM_PACKAGE_TEST_INDEX_URL: &str =
    "https://raw.githubusercontent.com/jlcodes99/cockpit-tools/platform-test/platform-packages/test/index.json";
const PLATFORM_PACKAGE_INDEX_CACHE_TTL_MS: i64 = 30 * 60 * 1000;
const MAX_PLATFORM_PACKAGE_DOWNLOAD_BYTES: u64 = 80 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackagePlatformContribution {
    pub id: String,
    pub label: String,
    pub label_key: Option<String>,
    pub icon_key: Option<String>,
    pub page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageContributions {
    #[serde(default)]
    pub platforms: Vec<PlatformPackagePlatformContribution>,
    #[serde(default)]
    pub data_paths: Vec<String>,
    #[serde(default)]
    pub local_storage_keys: Vec<String>,
    #[serde(default)]
    pub native_boundaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageAdapter {
    pub protocol: String,
    pub entry: String,
    #[serde(default)]
    pub macos_entry: Option<String>,
    #[serde(default)]
    pub windows_entry: Option<String>,
    #[serde(default)]
    pub linux_entry: Option<String>,
    #[serde(default)]
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageUi {
    pub protocol: String,
    pub entry: String,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageChangelogLocale {
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageChangelogEntry {
    pub version: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub locales: HashMap<String, PlatformPackageChangelogLocale>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageManifest {
    id: String,
    platform_id: String,
    version: String,
    api_version: u32,
    min_core_version: String,
    display_name: String,
    entry: String,
    package_mode: String,
    install_kind: String,
    #[serde(default)]
    adapter: Option<PlatformPackageAdapter>,
    #[serde(default)]
    ui: Option<PlatformPackageUi>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    contributions: PlatformPackageContributions,
    #[serde(default)]
    changelog: Vec<PlatformPackageChangelogEntry>,
    #[serde(default)]
    download_size_bytes: Option<u64>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageRuntimeEntry {
    package_id: String,
    platform_id: String,
    api_version: u32,
    #[serde(default)]
    adapter: Option<PlatformPackageAdapter>,
    #[serde(default)]
    ui: Option<PlatformPackageUi>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    contributions: PlatformPackageContributions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageState {
    pub platform_id: String,
    pub package_mode: String,
    pub install_kind: String,
    pub install_status: String,
    pub runtime_ready: bool,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub download_size_bytes: Option<u64>,
    pub installed_size_bytes: Option<u64>,
    pub last_checked_at: Option<i64>,
    pub error_message: Option<String>,
    pub entry: Option<String>,
    pub adapter: Option<PlatformPackageAdapter>,
    pub ui: Option<PlatformPackageUi>,
    pub capabilities: Vec<String>,
    pub contributions: PlatformPackageContributions,
    #[serde(default)]
    pub changelog: Vec<PlatformPackageChangelogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageRemoteIndex {
    #[serde(default)]
    version: String,
    #[serde(default)]
    packages: Vec<PlatformPackageRemotePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageRemoteArtifact {
    os: String,
    arch: String,
    download_url: String,
    #[serde(default)]
    download_size_bytes: Option<u64>,
    sha256: String,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Debug, Clone)]
struct SelectedPlatformPackageArtifact {
    os: String,
    arch: String,
    download_url: String,
    download_size_bytes: Option<u64>,
    sha256: String,
    signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageRemotePackage {
    id: String,
    platform_id: String,
    version: String,
    api_version: u32,
    min_core_version: String,
    display_name: String,
    entry: String,
    package_mode: String,
    install_kind: String,
    #[serde(default)]
    adapter: Option<PlatformPackageAdapter>,
    #[serde(default)]
    ui: Option<PlatformPackageUi>,
    capabilities: Vec<String>,
    #[serde(default)]
    contributions: PlatformPackageContributions,
    #[serde(default)]
    changelog: Vec<PlatformPackageChangelogEntry>,
    #[serde(default)]
    artifacts: Vec<PlatformPackageRemoteArtifact>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    download_size_bytes: Option<u64>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPackageUiEntry {
    pub platform_id: String,
    pub version: String,
    pub protocol: String,
    pub entry: String,
    #[serde(default)]
    pub exports: Vec<String>,
    pub sandbox: Option<String>,
    pub source: String,
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageIndexCache {
    time: i64,
    data: PlatformPackageRemoteIndex,
}

#[derive(Debug, Clone)]
enum PlatformPackageSource {
    Local {
        dir: PathBuf,
        manifest: PlatformPackageManifest,
    },
    Remote {
        package: PlatformPackageRemotePackage,
        manifest: PlatformPackageManifest,
    },
}

impl PlatformPackageSource {
    fn manifest(&self) -> &PlatformPackageManifest {
        match self {
            PlatformPackageSource::Local { manifest, .. }
            | PlatformPackageSource::Remote { manifest, .. } => manifest,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PlatformPackageRegistry {
    #[serde(default)]
    packages: Vec<PersistedPlatformPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedPlatformPackage {
    platform_id: String,
    installed: bool,
    runtime_ready: bool,
    installed_version: Option<String>,
    last_checked_at: Option<i64>,
    error_message: Option<String>,
}

fn now_ts_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn data_dir() -> Result<PathBuf, String> {
    crate::modules::app_data::get_data_dir()
}

fn registry_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join(PLATFORM_PACKAGE_REGISTRY_FILE))
}

fn index_cache_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join(PLATFORM_PACKAGE_INDEX_CACHE_FILE))
}

fn packages_root() -> Result<PathBuf, String> {
    let root = data_dir()?.join(PLATFORM_PACKAGE_DIR);
    fs::create_dir_all(&root).map_err(|err| format!("创建平台包根目录失败: {}", err))?;
    Ok(root)
}

fn package_dir(platform_id: &str) -> Result<PathBuf, String> {
    ensure_supported_platform(platform_id)?;
    Ok(packages_root()?.join(platform_id))
}

fn package_current_dir(platform_id: &str) -> Result<PathBuf, String> {
    Ok(package_dir(platform_id)?.join(CURRENT_DIR))
}

fn package_downloads_dir(platform_id: &str) -> Result<PathBuf, String> {
    let dir = package_dir(platform_id)?.join(DOWNLOADS_DIR);
    fs::create_dir_all(&dir).map_err(|err| format!("创建平台包下载缓存目录失败: {}", err))?;
    Ok(dir)
}

fn ensure_supported_platform(platform_id: &str) -> Result<(), String> {
    if SUPPORTED_PLATFORM_IDS.contains(&platform_id) {
        Ok(())
    } else {
        Err(format!("平台暂不支持热更新包: {}", platform_id))
    }
}

fn read_registry() -> Result<PlatformPackageRegistry, String> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(PlatformPackageRegistry::default());
    }

    let content = fs::read_to_string(&path).map_err(|err| {
        format!(
            "读取平台包注册表失败: path={}, error={}",
            path.display(),
            err
        )
    })?;
    atomic_write::parse_json_with_auto_restore(&path, &content)
        .map_err(|err| format!("解析平台包注册表失败: {}", err))
}

fn write_registry(registry: &PlatformPackageRegistry) -> Result<(), String> {
    let path = registry_path()?;
    let content = serde_json::to_string_pretty(registry)
        .map_err(|err| format!("序列化平台包注册表失败: {}", err))?;
    atomic_write::write_string_atomic(&path, &(content + "\n"))
}

fn upsert_record(registry: &mut PlatformPackageRegistry, record: PersistedPlatformPackage) {
    if let Some(existing) = registry
        .packages
        .iter_mut()
        .find(|item| item.platform_id == record.platform_id)
    {
        *existing = record;
        return;
    }
    registry.packages.push(record);
}

fn get_record<'a>(
    registry: &'a PlatformPackageRegistry,
    platform_id: &str,
) -> Option<&'a PersistedPlatformPackage> {
    registry
        .packages
        .iter()
        .find(|item| item.platform_id == platform_id)
}

fn dir_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| dir_size(&entry.path()))
        .sum::<u64>()
}

fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| format!("读取路径元数据失败: path={}, error={}", path.display(), err))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|err| format!("删除目录失败: path={}, error={}", path.display(), err))
    } else {
        fs::remove_file(path)
            .map_err(|err| format!("删除文件失败: path={}, error={}", path.display(), err))
    }
}

fn unique_work_dir(parent: &Path, prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{}.{}.{}", prefix, std::process::id(), unique))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>()
}

fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|err| {
        format!(
            "打开平台包下载文件失败: path={}, error={}",
            path.display(),
            err
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 256];
    loop {
        let read = io::Read::read(&mut file, &mut buffer).map_err(|err| {
            format!(
                "读取平台包下载文件失败: path={}, error={}",
                path.display(),
                err
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn sha256_package_file_hex(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|err| format!("打开平台包文件失败: path={}, error={}", path.display(), err))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 256];
    loop {
        let read = io::Read::read(&mut file, &mut buffer)
            .map_err(|err| format!("读取平台包文件失败: path={}, error={}", path.display(), err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn normalized_relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|err| {
        format!(
            "计算平台包相对路径失败: root={}, path={}, error={}",
            root.display(),
            path.display(),
            err
        )
    })?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/"))
}

fn collect_package_file_fingerprints(
    root: &Path,
    dir: &Path,
    output: &mut Vec<String>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("读取平台包目录失败: path={}, error={}", dir.display(), err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "读取平台包目录项失败: path={}, error={}",
                dir.display(),
                err
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|err| {
            format!(
                "读取平台包文件元数据失败: path={}, error={}",
                path.display(),
                err
            )
        })?;
        if metadata.is_dir() {
            collect_package_file_fingerprints(root, &path, output)?;
        } else if metadata.is_file() {
            let relative = normalized_relative_path(root, &path)?;
            let sha256 = sha256_package_file_hex(&path)?;
            output.push(format!("{}\t{}\t{}", relative, metadata.len(), sha256));
        }
    }

    Ok(())
}

fn package_dir_fingerprint(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_package_file_fingerprints(root, root, &mut files)?;
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.as_bytes());
        hasher.update(b"\n");
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn strict_local_source_validation_enabled() -> bool {
    cfg!(debug_assertions)
        || std::env::var("COCKPIT_PLATFORM_PACKAGE_STRICT_LOCAL_SOURCE")
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                normalized == "1" || normalized == "true" || normalized == "yes"
            })
            .unwrap_or(false)
}

fn validate_remote_download_url(raw: &str) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|err| format!("平台包下载 URL 非法: {}", err))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if cfg!(debug_assertions) => Ok(()),
        _ => Err("平台包下载 URL 必须使用 https".to_string()),
    }
}

fn safe_relative_path(raw: &str, context: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{} 为空", context));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(format!("{} 不能是绝对路径: {}", context, raw));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("{} 包含不安全路径片段: {}", context, raw));
    }
    Ok(path.to_path_buf())
}

fn read_manifest(path: &Path) -> Result<PlatformPackageManifest, String> {
    let content = fs::read_to_string(path)
        .map_err(|err| format!("读取平台包清单失败: path={}, error={}", path.display(), err))?;
    atomic_write::parse_json_with_auto_restore(path, &content)
        .map_err(|err| format!("解析平台包清单失败: {}", err))
}

fn read_runtime_entry(path: &Path) -> Result<PlatformPackageRuntimeEntry, String> {
    let content = fs::read_to_string(path).map_err(|err| {
        format!(
            "读取平台包 runtime 失败: path={}, error={}",
            path.display(),
            err
        )
    })?;
    atomic_write::parse_json_with_auto_restore(path, &content)
        .map_err(|err| format!("解析平台包 runtime 失败: {}", err))
}

fn parse_version(value: &str) -> Vec<u64> {
    value
        .trim()
        .split(|ch| ch == '.' || ch == '-' || ch == '+')
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let mut left_parts = parse_version(left);
    let mut right_parts = parse_version(right);
    left_parts.resize(3, 0);
    right_parts.resize(3, 0);
    left_parts.cmp(&right_parts)
}

fn validate_platform_contributions(
    platform_id: &str,
    contributions: &PlatformPackageContributions,
) -> Result<(), String> {
    if !contributions
        .platforms
        .iter()
        .any(|platform| platform.id == platform_id)
    {
        return Err(format!("平台包缺少平台贡献: {}", platform_id));
    }

    for platform in &contributions.platforms {
        if platform.id != platform_id {
            return Err(format!("平台包贡献包含非本平台 ID: {}", platform.id));
        }
        if platform.label.trim().is_empty() {
            return Err("平台包贡献 label 为空".to_string());
        }
        if platform.page.trim().is_empty() {
            return Err("平台包贡献 page 为空".to_string());
        }
    }

    for path in &contributions.data_paths {
        safe_relative_path(path, "平台包 dataPath")?;
    }
    for path in &contributions.native_boundaries {
        safe_relative_path(path, "平台包 nativeBoundary")?;
    }
    for key in &contributions.local_storage_keys {
        if key.trim().is_empty() {
            return Err("平台包 localStorage key 为空".to_string());
        }
    }

    Ok(())
}

fn validate_manifest_metadata(
    platform_id: &str,
    manifest: &PlatformPackageManifest,
) -> Result<(), String> {
    ensure_supported_platform(platform_id)?;
    if manifest.id != platform_id || manifest.platform_id != platform_id {
        return Err(format!(
            "平台包 ID 不匹配: expected={}, id={}, platformId={}",
            platform_id, manifest.id, manifest.platform_id
        ));
    }
    if manifest.package_mode != "hotUpdate" {
        return Err(format!("平台包模式非法: {}", manifest.package_mode));
    }
    if manifest.install_kind != "coreNativeBoundary" && manifest.install_kind != "sidecarAdapter" {
        return Err(format!("平台包安装形态非法: {}", manifest.install_kind));
    }
    if manifest.api_version != PLATFORM_PACKAGE_API_VERSION {
        return Err(format!(
            "平台包协议版本不兼容: expected={}, actual={}",
            PLATFORM_PACKAGE_API_VERSION, manifest.api_version
        ));
    }
    if manifest.version.trim().is_empty() {
        return Err("平台包版本为空".to_string());
    }
    if compare_versions(env!("CARGO_PKG_VERSION"), &manifest.min_core_version) == Ordering::Less {
        return Err(format!(
            "主应用版本不兼容，平台包需要 {} 或更高版本",
            manifest.min_core_version
        ));
    }
    if manifest.capabilities.is_empty() {
        return Err("平台包 capabilities 为空".to_string());
    }
    validate_platform_contributions(platform_id, &manifest.contributions)?;

    Ok(())
}

pub fn selected_adapter_entry(adapter: &PlatformPackageAdapter) -> &str {
    #[cfg(target_os = "macos")]
    {
        adapter.macos_entry.as_deref().unwrap_or(&adapter.entry)
    }
    #[cfg(target_os = "windows")]
    {
        adapter.windows_entry.as_deref().unwrap_or(&adapter.entry)
    }
    #[cfg(target_os = "linux")]
    {
        adapter.linux_entry.as_deref().unwrap_or(&adapter.entry)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        &adapter.entry
    }
}

fn validate_adapter(
    platform_id: &str,
    package_root: &Path,
    manifest: &PlatformPackageManifest,
) -> Result<(), String> {
    let Some(adapter) = manifest.adapter.as_ref() else {
        if manifest.install_kind == "sidecarAdapter" {
            return Err(format!(
                "sidecarAdapter 平台包缺少 adapter 声明: {}",
                platform_id
            ));
        }
        return Ok(());
    };

    if adapter.protocol.trim() != "http-json-v1" {
        return Err(format!("平台包 adapter 协议不支持: {}", adapter.protocol));
    }
    if adapter.methods.is_empty() {
        return Err("平台包 adapter methods 为空".to_string());
    }
    let entry = selected_adapter_entry(adapter);
    let entry_path = safe_relative_path(entry, "平台包 adapter entry")?;
    let adapter_path = package_root.join(entry_path);
    if !adapter_path.is_file() {
        return Err(format!("平台包 adapter entry 不存在: {}", entry));
    }
    Ok(())
}

fn validate_ui(package_root: &Path, manifest: &PlatformPackageManifest) -> Result<(), String> {
    let Some(ui) = manifest.ui.as_ref() else {
        return Ok(());
    };

    let protocol = ui.protocol.trim();
    let entry_path = safe_relative_path(&ui.entry, "平台包 UI entry")?;
    let ui_path = package_root.join(entry_path);
    if !ui_path.is_file() {
        return Err(format!("平台包 UI entry 不存在: {}", ui.entry));
    }
    let extension = ui_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    match protocol {
        "react-remote-esm-v1" => {
            if !extension.eq_ignore_ascii_case("js") && !extension.eq_ignore_ascii_case("mjs") {
                return Err(format!(
                    "平台包 remote UI entry 必须是 JS/MJS 文件: {}",
                    ui.entry
                ));
            }
            if !ui.exports.iter().any(|item| item == "mount") {
                return Err("平台包 remote UI 必须声明 mount 导出".to_string());
            }
            if let Some(style) = ui.style.as_deref() {
                let style_path = safe_relative_path(style, "平台包 UI style")?;
                let style_path = package_root.join(style_path);
                if !style_path.is_file() {
                    return Err(format!("平台包 UI style 不存在: {}", style));
                }
                if style_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.eq_ignore_ascii_case("css"))
                    != Some(true)
                {
                    return Err(format!("平台包 UI style 必须是 CSS 文件: {}", style));
                }
            }
        }
        "iframe-html-v1" => {
            if !extension.eq_ignore_ascii_case("html") {
                return Err(format!("平台包 UI entry 必须是 HTML 文件: {}", ui.entry));
            }
            if let Some(sandbox) = ui.sandbox.as_deref() {
                let allowed = [
                    "allow-scripts",
                    "allow-forms",
                    "allow-popups",
                    "allow-downloads",
                    "allow-modals",
                ];
                for token in sandbox.split_whitespace() {
                    if !allowed.contains(&token) {
                        return Err(format!("平台包 UI sandbox 权限不支持: {}", token));
                    }
                }
            }
        }
        _ => return Err(format!("平台包 UI 协议不支持: {}", ui.protocol)),
    }
    Ok(())
}

fn validate_manifest(
    platform_id: &str,
    package_root: &Path,
) -> Result<PlatformPackageManifest, String> {
    ensure_supported_platform(platform_id)?;
    let manifest_path = package_root.join(MANIFEST_FILE);
    let manifest = read_manifest(&manifest_path)?;
    validate_manifest_metadata(platform_id, &manifest)?;

    let entry_path = safe_relative_path(&manifest.entry, "平台包 entry")?;
    let runtime_path = package_root.join(entry_path);
    if !runtime_path.exists() {
        return Err(format!("平台包 runtime entry 不存在: {}", manifest.entry));
    }

    let runtime = read_runtime_entry(&runtime_path)?;
    if runtime.package_id != manifest.id || runtime.platform_id != manifest.platform_id {
        return Err("平台包 manifest 与 runtime ID 不一致".to_string());
    }
    if runtime.api_version != manifest.api_version {
        return Err("平台包 manifest 与 runtime 协议版本不一致".to_string());
    }
    if runtime.capabilities != manifest.capabilities {
        return Err("平台包 manifest 与 runtime capabilities 不一致".to_string());
    }
    if runtime.adapter != manifest.adapter {
        return Err("平台包 manifest 与 runtime adapter 声明不一致".to_string());
    }
    if runtime.ui != manifest.ui {
        return Err("平台包 manifest 与 runtime UI 声明不一致".to_string());
    }
    if runtime.contributions != manifest.contributions {
        return Err("平台包 manifest 与 runtime contribution 不一致".to_string());
    }
    validate_adapter(platform_id, package_root, &manifest)?;
    validate_ui(package_root, &manifest)?;

    Ok(manifest)
}

fn read_installed_manifest(platform_id: &str) -> Result<Option<PlatformPackageManifest>, String> {
    let current_dir = package_current_dir(platform_id)?;
    if !current_dir.join(MANIFEST_FILE).exists() {
        return Ok(None);
    }
    validate_manifest(platform_id, &current_dir).map(Some)
}

#[derive(Debug, Clone)]
pub struct InstalledPlatformAdapter {
    pub current_dir: PathBuf,
    pub adapter: PlatformPackageAdapter,
    pub executable_path: PathBuf,
}

pub fn installed_platform_adapter(platform_id: &str) -> Result<InstalledPlatformAdapter, String> {
    ensure_platform_package_installed(platform_id)?;
    let current_dir = package_current_dir(platform_id)?;
    let manifest = read_installed_manifest(platform_id)?
        .ok_or_else(|| format!("平台包未安装: {}", platform_id))?;
    let adapter = manifest
        .adapter
        .clone()
        .ok_or_else(|| format!("平台包缺少 adapter 声明: {}", platform_id))?;
    let entry = selected_adapter_entry(&adapter);
    let entry_path = safe_relative_path(entry, "平台包 adapter entry")?;
    let executable_path = current_dir.join(entry_path);
    if !executable_path.is_file() {
        return Err(format!("平台包 adapter entry 不存在: {}", entry));
    }
    Ok(InstalledPlatformAdapter {
        current_dir,
        adapter,
        executable_path,
    })
}

fn source_package_dir_candidates(app: &AppHandle, platform_id: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join(PLATFORM_PACKAGE_DIR).join(platform_id));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(repo_root) = manifest_dir.parent() {
        candidates.push(repo_root.join(PLATFORM_PACKAGE_DIR).join(platform_id));
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(PLATFORM_PACKAGE_DIR).join(platform_id));
        candidates.push(
            current_dir
                .join("..")
                .join(PLATFORM_PACKAGE_DIR)
                .join(platform_id),
        );
    }

    candidates
}

fn resolve_source_package_dir(app: &AppHandle, platform_id: &str) -> Result<PathBuf, String> {
    ensure_supported_platform(platform_id)?;
    for candidate in source_package_dir_candidates(app, platform_id) {
        if candidate.join(MANIFEST_FILE).exists() {
            return Ok(candidate);
        }
    }
    Err(format!("未找到平台包源: {}", platform_id))
}

fn read_local_source(app: &AppHandle, platform_id: &str) -> Option<PlatformPackageSource> {
    let dir = resolve_source_package_dir(app, platform_id).ok()?;
    let manifest = validate_manifest(platform_id, &dir).ok()?;
    Some(PlatformPackageSource::Local { dir, manifest })
}

fn platform_package_index_url() -> String {
    std::env::var("COCKPIT_PLATFORM_PACKAGE_INDEX_URL")
        .ok()
        .or_else(|| option_env!("COCKPIT_PLATFORM_PACKAGE_INDEX_URL").map(ToString::to_string))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if crate::modules::app_data::is_test_profile() {
                PLATFORM_PACKAGE_TEST_INDEX_URL.to_string()
            } else {
                PLATFORM_PACKAGE_INDEX_URL.to_string()
            }
        })
}

fn workspace_package_index_candidates() -> Vec<PathBuf> {
    if !cfg!(debug_assertions) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    if let Ok(data_dir) = data_dir() {
        candidates.push(data_dir.join(PLATFORM_PACKAGE_INDEX_LOCAL_OVERRIDE_FILE));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(repo_root) = manifest_dir.parent() {
        candidates.push(
            repo_root
                .join(PLATFORM_PACKAGE_DIR)
                .join("index.local.json"),
        );
        candidates.push(repo_root.join(PLATFORM_PACKAGE_DIR).join("index.json"));
    }

    candidates
}

fn parse_remote_index_file(path: &Path) -> Result<PlatformPackageRemoteIndex, String> {
    let content = fs::read_to_string(path).map_err(|err| {
        format!(
            "读取平台包远端索引失败: path={}, error={}",
            path.display(),
            err
        )
    })?;
    atomic_write::parse_json_with_auto_restore(path, &content)
        .map_err(|err| format!("解析平台包远端索引失败: {}", err))
}

fn load_local_remote_index() -> Result<Option<PlatformPackageRemoteIndex>, String> {
    for candidate in workspace_package_index_candidates() {
        if candidate.exists() {
            logger::log_info(&format!(
                "[PlatformPackage] 使用本地平台包索引: {}",
                candidate.display()
            ));
            return parse_remote_index_file(&candidate).map(Some);
        }
    }
    Ok(None)
}

fn read_index_cache() -> Result<Option<PlatformPackageIndexCache>, String> {
    let path = index_cache_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|err| {
        format!(
            "读取平台包索引缓存失败: path={}, error={}",
            path.display(),
            err
        )
    })?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    atomic_write::parse_json_with_auto_restore(&path, &content)
        .map(Some)
        .map_err(|err| format!("解析平台包索引缓存失败: {}", err))
}

fn write_index_cache(index: &PlatformPackageRemoteIndex) -> Result<(), String> {
    let path = index_cache_path()?;
    let cache = PlatformPackageIndexCache {
        time: now_ts_ms(),
        data: index.clone(),
    };
    let content = serde_json::to_string_pretty(&cache)
        .map_err(|err| format!("序列化平台包索引缓存失败: {}", err))?;
    atomic_write::write_string_atomic(&path, &(content + "\n"))
}

fn fetch_remote_index() -> Result<PlatformPackageRemoteIndex, String> {
    let url = platform_package_index_url();
    validate_remote_download_url(&url)?;
    logger::log_info(&format!("[PlatformPackage] 拉取远端平台包索引: {}", url));
    let client = reqwest::blocking::Client::builder()
        .user_agent("Cockpit-Tools")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("创建平台包索引 HTTP 客户端失败: {}", err))?;
    let response = client
        .get(&url)
        .header("Cache-Control", "no-cache")
        .header("Pragma", "no-cache")
        .send()
        .map_err(|err| format!("拉取平台包索引失败: {}", err))?;
    if !response.status().is_success() {
        return Err(format!(
            "平台包索引返回异常状态: HTTP {} ({})",
            response.status(),
            url
        ));
    }
    response
        .json::<PlatformPackageRemoteIndex>()
        .map_err(|err| format!("解析平台包索引响应失败: {}", err))
}

fn load_remote_index(force_refresh: bool) -> Result<Option<PlatformPackageRemoteIndex>, String> {
    if let Some(index) = load_local_remote_index()? {
        return Ok(Some(index));
    }

    if !force_refresh {
        if let Some(cache) = read_index_cache()? {
            let fresh =
                now_ts_ms().saturating_sub(cache.time) <= PLATFORM_PACKAGE_INDEX_CACHE_TTL_MS;
            if fresh {
                return Ok(Some(cache.data));
            }
        }
    }

    match fetch_remote_index() {
        Ok(index) => {
            if let Err(err) = write_index_cache(&index) {
                logger::log_warn(&format!(
                    "[PlatformPackage] 写入平台包索引缓存失败，继续使用远端结果: {}",
                    err
                ));
            }
            Ok(Some(index))
        }
        Err(error) => {
            logger::log_warn(&format!(
                "[PlatformPackage] 拉取远端平台包索引失败，尝试使用缓存或内置包: {}",
                error
            ));
            read_index_cache().map(|cache| cache.map(|item| item.data))
        }
    }
}

fn current_artifact_os() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unknown"
    }
}

fn current_artifact_arch() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "arm")]
    {
        "arm"
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64", target_arch = "arm")))]
    {
        "unknown"
    }
}

fn validate_sha256_hex(platform_id: &str, sha256: &str) -> Result<(), String> {
    if sha256.trim().len() != 64 || !sha256.trim().chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!("平台包远端索引 sha256 非法: {}", platform_id));
    }
    Ok(())
}

fn selected_remote_artifact(
    platform_id: &str,
    package: &PlatformPackageRemotePackage,
) -> Result<SelectedPlatformPackageArtifact, String> {
    let target_os = current_artifact_os();
    let target_arch = current_artifact_arch();

    if !package.artifacts.is_empty() {
        let artifact = package
            .artifacts
            .iter()
            .find(|item| item.os == target_os && item.arch == target_arch)
            .ok_or_else(|| {
                format!(
                    "当前平台没有可用平台包 artifact: platform={}, target={}/{}",
                    platform_id, target_os, target_arch
                )
            })?;
        validate_remote_download_url(&artifact.download_url)?;
        validate_sha256_hex(platform_id, &artifact.sha256)?;
        return Ok(SelectedPlatformPackageArtifact {
            os: artifact.os.clone(),
            arch: artifact.arch.clone(),
            download_url: artifact.download_url.clone(),
            download_size_bytes: artifact.download_size_bytes.or(package.download_size_bytes),
            sha256: artifact.sha256.clone(),
            signature: artifact
                .signature
                .clone()
                .or_else(|| package.signature.clone()),
        });
    }

    let download_url = package
        .download_url
        .clone()
        .ok_or_else(|| format!("平台包远端索引缺少 downloadUrl: {}", platform_id))?;
    let sha256 = package
        .sha256
        .clone()
        .ok_or_else(|| format!("平台包远端索引缺少 sha256: {}", platform_id))?;
    validate_remote_download_url(&download_url)?;
    validate_sha256_hex(platform_id, &sha256)?;
    Ok(SelectedPlatformPackageArtifact {
        os: target_os.to_string(),
        arch: target_arch.to_string(),
        download_url,
        download_size_bytes: package.download_size_bytes,
        sha256,
        signature: package.signature.clone(),
    })
}

fn manifest_from_remote_package(
    platform_id: &str,
    package: &PlatformPackageRemotePackage,
) -> Result<PlatformPackageManifest, String> {
    ensure_supported_platform(platform_id)?;
    let artifact = selected_remote_artifact(platform_id, package)?;

    let manifest = PlatformPackageManifest {
        id: package.id.clone(),
        platform_id: package.platform_id.clone(),
        version: package.version.clone(),
        api_version: package.api_version,
        min_core_version: package.min_core_version.clone(),
        display_name: package.display_name.clone(),
        entry: package.entry.clone(),
        package_mode: package.package_mode.clone(),
        install_kind: package.install_kind.clone(),
        adapter: package.adapter.clone(),
        ui: package.ui.clone(),
        capabilities: package.capabilities.clone(),
        contributions: package.contributions.clone(),
        changelog: package.changelog.clone(),
        download_size_bytes: artifact.download_size_bytes,
        sha256: Some(artifact.sha256),
        signature: artifact.signature,
    };
    validate_manifest_metadata(platform_id, &manifest)?;
    Ok(manifest)
}

fn read_remote_source(platform_id: &str, force_refresh: bool) -> Option<PlatformPackageSource> {
    let index = match load_remote_index(force_refresh) {
        Ok(Some(index)) => index,
        Ok(None) => return None,
        Err(error) => {
            logger::log_warn(&format!(
                "[PlatformPackage] 平台包索引不可用，忽略远端源: {}",
                error
            ));
            return None;
        }
    };

    let package = index
        .packages
        .into_iter()
        .find(|item| item.platform_id == platform_id || item.id == platform_id)?;
    match manifest_from_remote_package(platform_id, &package) {
        Ok(manifest) => Some(PlatformPackageSource::Remote { package, manifest }),
        Err(error) => {
            logger::log_warn(&format!(
                "[PlatformPackage] 平台包远端索引项无效，忽略远端源: platform={}, error={}",
                platform_id, error
            ));
            None
        }
    }
}

fn pick_latest_source(
    remote: Option<PlatformPackageSource>,
    local: Option<PlatformPackageSource>,
) -> Option<PlatformPackageSource> {
    match (remote, local) {
        (Some(remote), Some(local)) => {
            if compare_versions(&remote.manifest().version, &local.manifest().version)
                == Ordering::Greater
            {
                Some(remote)
            } else {
                Some(local)
            }
        }
        (Some(remote), None) => Some(remote),
        (None, Some(local)) => Some(local),
        (None, None) => None,
    }
}

fn resolve_package_source(
    app: &AppHandle,
    platform_id: &str,
    force_remote_refresh: bool,
) -> Result<PlatformPackageSource, String> {
    ensure_supported_platform(platform_id)?;
    let remote = read_remote_source(platform_id, force_remote_refresh);
    let local = read_local_source(app, platform_id);
    pick_latest_source(remote, local).ok_or_else(|| format!("未找到平台包源: {}", platform_id))
}

fn read_latest_source_manifest(
    app: &AppHandle,
    platform_id: &str,
    force_remote_refresh: bool,
) -> Option<PlatformPackageManifest> {
    read_latest_source_manifest_and_root(app, platform_id, force_remote_refresh).0
}

fn read_latest_source_manifest_and_root(
    app: &AppHandle,
    platform_id: &str,
    force_remote_refresh: bool,
) -> (Option<PlatformPackageManifest>, Option<PathBuf>) {
    match resolve_package_source(app, platform_id, force_remote_refresh).ok() {
        Some(PlatformPackageSource::Local { dir, manifest }) => (Some(manifest), Some(dir)),
        Some(PlatformPackageSource::Remote { manifest, .. }) => (Some(manifest), None),
        None => (None, None),
    }
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        fs::remove_dir_all(target).map_err(|err| format!("清理旧平台包目录失败: {}", err))?;
    }
    fs::create_dir_all(target).map_err(|err| format!("创建平台包目标目录失败: {}", err))?;

    for entry in fs::read_dir(source).map_err(|err| format!("读取平台包源目录失败: {}", err))?
    {
        let entry = entry.map_err(|err| format!("读取平台包源目录项失败: {}", err))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("读取平台包文件类型失败: {}", err))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path)
                .map_err(|err| format!("复制平台包文件失败: {}", err))?;
        }
    }

    Ok(())
}

fn replace_current_with_prepared(
    platform_id: &str,
    prepared_root: &Path,
) -> Result<PlatformPackageManifest, String> {
    let installed_manifest = validate_manifest(platform_id, prepared_root)?;
    let platform_dir = package_dir(platform_id)?;
    fs::create_dir_all(&platform_dir).map_err(|err| format!("创建平台包目录失败: {}", err))?;

    let current_dir = platform_dir.join(CURRENT_DIR);
    let backup_dir = unique_work_dir(&platform_dir, "previous");
    if backup_dir.exists() {
        remove_path_if_exists(&backup_dir)?;
    }

    if current_dir.exists() {
        fs::rename(&current_dir, &backup_dir).map_err(|err| {
            format!(
                "备份旧平台包目录失败: from={}, to={}, error={}",
                current_dir.display(),
                backup_dir.display(),
                err
            )
        })?;
    }

    let prepared_parent = prepared_root.parent().map(Path::to_path_buf);
    if let Err(err) = fs::rename(prepared_root, &current_dir) {
        if backup_dir.exists() {
            let _ = fs::rename(&backup_dir, &current_dir);
        }
        return Err(format!(
            "切换平台包目录失败: from={}, to={}, error={}",
            prepared_root.display(),
            current_dir.display(),
            err
        ));
    }

    if backup_dir.exists() {
        let _ = remove_path_if_exists(&backup_dir);
    }
    if let Some(parent) = prepared_parent {
        if parent != platform_dir && parent.exists() {
            let _ = remove_path_if_exists(&parent);
        }
    }

    validate_manifest(platform_id, &current_dir).map(|_| installed_manifest)
}

fn install_local_source(
    platform_id: &str,
    source_dir: &Path,
) -> Result<PlatformPackageManifest, String> {
    let platform_dir = package_dir(platform_id)?;
    let staging_dir = unique_work_dir(&platform_dir, "staging");
    remove_path_if_exists(&staging_dir)?;
    copy_dir_all(source_dir, &staging_dir)?;
    match replace_current_with_prepared(platform_id, &staging_dir) {
        Ok(manifest) => Ok(manifest),
        Err(error) => {
            let _ = remove_path_if_exists(&staging_dir);
            Err(error)
        }
    }
}

fn download_remote_package_zip(
    platform_id: &str,
    package: &PlatformPackageRemotePackage,
) -> Result<PathBuf, String> {
    let artifact = selected_remote_artifact(platform_id, package)?;
    let downloads_dir = package_downloads_dir(platform_id)?;
    let zip_path = downloads_dir.join(format!(
        "{}-{}-{}-{}.zip",
        platform_id, package.version, artifact.os, artifact.arch
    ));
    let expected_sha256 = artifact.sha256.trim().to_ascii_lowercase();

    if zip_path.exists() {
        match sha256_file_hex(&zip_path) {
            Ok(actual) if actual.eq_ignore_ascii_case(&expected_sha256) => {
                logger::log_info(&format!(
                    "[PlatformPackage] 使用已缓存平台包: platform={}, path={}",
                    platform_id,
                    zip_path.display()
                ));
                return Ok(zip_path);
            }
            Ok(actual) => {
                logger::log_warn(&format!(
                    "[PlatformPackage] 平台包缓存校验失败，重新下载: platform={}, expected={}, actual={}",
                    platform_id, expected_sha256, actual
                ));
                let _ = remove_path_if_exists(&zip_path);
            }
            Err(error) => {
                logger::log_warn(&format!(
                    "[PlatformPackage] 平台包缓存读取失败，重新下载: platform={}, error={}",
                    platform_id, error
                ));
                let _ = remove_path_if_exists(&zip_path);
            }
        }
    }

    logger::log_info(&format!(
        "[PlatformPackage] 下载远端平台包: platform={}, url={}",
        platform_id, artifact.download_url
    ));
    let client = reqwest::blocking::Client::builder()
        .user_agent("Cockpit-Tools")
        .timeout(Duration::from_secs(10 * 60))
        .build()
        .map_err(|err| format!("创建平台包下载 HTTP 客户端失败: {}", err))?;
    let mut response = client
        .get(&artifact.download_url)
        .send()
        .map_err(|err| format!("下载平台包失败: {}", err))?;
    if !response.status().is_success() {
        return Err(format!(
            "下载平台包失败: HTTP {} ({})",
            response.status(),
            artifact.download_url
        ));
    }

    let temp_path = zip_path.with_extension("zip.part");
    let mut temp_file = File::create(&temp_path).map_err(|err| {
        format!(
            "创建平台包下载临时文件失败: path={}, error={}",
            temp_path.display(),
            err
        )
    })?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 1024 * 256];
    loop {
        let read = io::Read::read(&mut response, &mut buffer)
            .map_err(|err| format!("读取平台包下载数据失败: {}", err))?;
        if read == 0 {
            break;
        }
        downloaded += read as u64;
        if downloaded > MAX_PLATFORM_PACKAGE_DOWNLOAD_BYTES {
            let _ = remove_path_if_exists(&temp_path);
            return Err("平台包下载内容超过预期大小，已停止".to_string());
        }
        hasher.update(&buffer[..read]);
        io::Write::write_all(&mut temp_file, &buffer[..read])
            .map_err(|err| format!("写入平台包下载临时文件失败: {}", err))?;
    }
    temp_file
        .sync_all()
        .map_err(|err| format!("同步平台包下载临时文件失败: {}", err))?;
    drop(temp_file);

    if let Some(expected_size) = artifact.download_size_bytes {
        if expected_size > 0 && expected_size != downloaded {
            let _ = remove_path_if_exists(&temp_path);
            return Err(format!(
                "平台包大小校验失败: expected={}, actual={}",
                expected_size, downloaded
            ));
        }
    }

    let actual_sha256 = hex_encode(&hasher.finalize());
    if !actual_sha256.eq_ignore_ascii_case(&expected_sha256) {
        let _ = remove_path_if_exists(&temp_path);
        return Err(format!(
            "平台包 sha256 校验失败: expected={}, actual={}",
            expected_sha256, actual_sha256
        ));
    }

    if zip_path.exists() {
        let _ = remove_path_if_exists(&zip_path);
    }
    fs::rename(&temp_path, &zip_path).map_err(|err| {
        format!(
            "保存平台包下载缓存失败: from={}, to={}, error={}",
            temp_path.display(),
            zip_path.display(),
            err
        )
    })?;
    Ok(zip_path)
}

fn extract_zip_safely(
    archive: &mut zip::ZipArchive<File>,
    target_dir: &Path,
) -> Result<(), String> {
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| format!("读取平台包 zip 条目失败: {}", err))?;
        let raw_name = file.name().to_string();
        let enclosed_name = file
            .enclosed_name()
            .ok_or_else(|| format!("平台包 zip 包含不安全路径: {}", raw_name))?;
        let output_path = target_dir.join(enclosed_name);

        if file.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(|err| format!("创建平台包解压目录失败: {}", err))?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("创建平台包解压父目录失败: {}", err))?;
        }
        let mut output_file = File::create(&output_path).map_err(|err| {
            format!(
                "创建平台包解压文件失败: path={}, error={}",
                output_path.display(),
                err
            )
        })?;
        io::copy(&mut file, &mut output_file)
            .map_err(|err| format!("写入平台包解压文件失败: {}", err))?;
        #[cfg(unix)]
        if let Some(mode) = file.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(mode);
            fs::set_permissions(&output_path, permissions)
                .map_err(|err| format!("设置平台包文件权限失败: {}", err))?;
        }
    }
    Ok(())
}

fn extract_remote_package_zip(platform_id: &str, zip_path: &Path) -> Result<PathBuf, String> {
    let platform_dir = package_dir(platform_id)?;
    let staging_dir = unique_work_dir(&platform_dir, "extracting");
    remove_path_if_exists(&staging_dir)?;
    fs::create_dir_all(&staging_dir).map_err(|err| format!("创建平台包解压目录失败: {}", err))?;

    let archive_file = File::open(zip_path).map_err(|err| {
        format!(
            "打开平台包压缩文件失败: path={}, error={}",
            zip_path.display(),
            err
        )
    })?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|err| format!("解析平台包 zip 失败: {}", err))?;
    extract_zip_safely(&mut archive, &staging_dir)?;

    if staging_dir.join(MANIFEST_FILE).exists() {
        return Ok(staging_dir);
    }

    let entries = fs::read_dir(&staging_dir)
        .map_err(|err| format!("读取平台包解压目录失败: {}", err))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    if entries.len() == 1 && entries[0].join(MANIFEST_FILE).exists() {
        return Ok(entries[0].clone());
    }

    let _ = remove_path_if_exists(&staging_dir);
    Err("平台包 zip 根目录缺少 manifest.json".to_string())
}

fn install_remote_source(
    platform_id: &str,
    package: &PlatformPackageRemotePackage,
) -> Result<PlatformPackageManifest, String> {
    let zip_path = download_remote_package_zip(platform_id, package)?;
    let prepared_root = extract_remote_package_zip(platform_id, &zip_path)?;
    match replace_current_with_prepared(platform_id, &prepared_root) {
        Ok(manifest) => Ok(manifest),
        Err(error) => {
            if prepared_root.exists() {
                let _ = remove_path_if_exists(&prepared_root);
            }
            if let (Ok(platform_dir), Some(parent)) =
                (package_dir(platform_id), prepared_root.parent())
            {
                if parent != platform_dir
                    && parent
                        .file_name()
                        .and_then(|item| item.to_str())
                        .map(|name| name.starts_with(".extracting."))
                        .unwrap_or(false)
                {
                    let _ = remove_path_if_exists(parent);
                }
            }
            Err(error)
        }
    }
}

fn build_state(
    platform_id: &str,
    record: Option<&PersistedPlatformPackage>,
    installed_manifest: Option<PlatformPackageManifest>,
    source_manifest: Option<PlatformPackageManifest>,
    source_root: Option<PathBuf>,
    validation_error: Option<String>,
) -> Result<PlatformPackageState, String> {
    ensure_supported_platform(platform_id)?;
    let current_dir = package_current_dir(platform_id)?;
    let installed = current_dir.join(MANIFEST_FILE).exists() && installed_manifest.is_some();
    let latest_version = source_manifest
        .as_ref()
        .map(|manifest| manifest.version.clone());
    let installed_version = installed_manifest
        .as_ref()
        .map(|manifest| manifest.version.clone())
        .or_else(|| record.and_then(|item| item.installed_version.clone()));
    let download_size_bytes = source_manifest
        .as_ref()
        .and_then(|manifest| manifest.download_size_bytes)
        .or_else(|| resolve_source_size_from_current_process(platform_id));
    let installed_size_bytes = if installed {
        Some(dir_size(&current_dir))
    } else {
        None
    };
    let runtime_contract_error = installed_manifest
        .as_ref()
        .zip(source_manifest.as_ref())
        .filter(|(installed, source)| same_version_runtime_contract_mismatch(installed, source))
        .map(|(installed, source)| {
            logger::log_warn(&format!(
                "[PlatformPackage] 运行契约不一致: platform={}, installedVersion={}, sourceVersion={}, {}",
                platform_id,
                installed.version,
                source.version,
                describe_runtime_contract_mismatch(installed, source)
            ));
            "已安装平台包与当前运行组件声明不一致，请修复或重新安装平台包".to_string()
        });
    let local_content_error = installed_manifest
        .as_ref()
        .zip(source_manifest.as_ref())
        .zip(source_root.as_ref())
        .and_then(|((installed, source), source_root)| {
            same_version_local_package_content_error(
                platform_id,
                installed,
                source,
                &current_dir,
                source_root,
            )
        });

    let mut runtime_ready = installed
        && validation_error.is_none()
        && runtime_contract_error.is_none()
        && local_content_error.is_none()
        && record.map(|item| item.runtime_ready).unwrap_or(false);
    let mut error_message = validation_error
        .or(runtime_contract_error)
        .or(local_content_error)
        .or_else(|| record.and_then(|item| item.error_message.clone()));
    if !installed {
        runtime_ready = false;
        if record.map(|item| item.installed).unwrap_or(false) {
            error_message.get_or_insert_with(|| "平台包文件缺失".to_string());
        } else {
            error_message = None;
        }
    }

    let manifest_for_meta = installed_manifest.as_ref().or(source_manifest.as_ref());
    let changelog = source_manifest
        .as_ref()
        .filter(|manifest| !manifest.changelog.is_empty())
        .or_else(|| {
            installed_manifest
                .as_ref()
                .filter(|manifest| !manifest.changelog.is_empty())
        })
        .map(|manifest| manifest.changelog.clone())
        .unwrap_or_default();
    let has_update = installed
        && runtime_ready
        && installed_version.is_some()
        && latest_version.is_some()
        && installed_version.as_ref() != latest_version.as_ref();
    let install_status = if error_message
        .as_deref()
        .map(|message| message.contains("主应用版本不兼容"))
        .unwrap_or(false)
    {
        "incompatible"
    } else if !installed {
        "notInstalled"
    } else if !runtime_ready {
        "error"
    } else if has_update {
        "updateAvailable"
    } else {
        "installed"
    };

    Ok(PlatformPackageState {
        platform_id: platform_id.to_string(),
        package_mode: manifest_for_meta
            .map(|manifest| manifest.package_mode.clone())
            .unwrap_or_else(|| "hotUpdate".to_string()),
        install_kind: manifest_for_meta
            .map(|manifest| manifest.install_kind.clone())
            .unwrap_or_else(|| "coreNativeBoundary".to_string()),
        install_status: install_status.to_string(),
        runtime_ready,
        installed_version: if installed { installed_version } else { None },
        latest_version,
        download_size_bytes,
        installed_size_bytes,
        last_checked_at: record.and_then(|item| item.last_checked_at),
        error_message,
        entry: manifest_for_meta.map(|manifest| manifest.entry.clone()),
        adapter: manifest_for_meta.and_then(|manifest| manifest.adapter.clone()),
        ui: manifest_for_meta.and_then(|manifest| manifest.ui.clone()),
        capabilities: manifest_for_meta
            .map(|manifest| manifest.capabilities.clone())
            .unwrap_or_default(),
        contributions: manifest_for_meta
            .map(|manifest| manifest.contributions.clone())
            .unwrap_or_default(),
        changelog,
    })
}

fn resolve_source_size_from_current_process(platform_id: &str) -> Option<u64> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent()?;
    let source_dir = repo_root.join(PLATFORM_PACKAGE_DIR).join(platform_id);
    source_dir.exists().then(|| dir_size(&source_dir))
}

fn local_source_package_dir_from_current_process(platform_id: &str) -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent()?;
    let source_dir = repo_root.join(PLATFORM_PACKAGE_DIR).join(platform_id);
    source_dir
        .join(MANIFEST_FILE)
        .exists()
        .then_some(source_dir)
}

fn runtime_contract_mismatch(
    installed: &PlatformPackageManifest,
    source: &PlatformPackageManifest,
) -> bool {
    installed.api_version != source.api_version
        || installed.install_kind != source.install_kind
        || installed.adapter != source.adapter
        || installed.ui != source.ui
        || installed.capabilities != source.capabilities
        || installed.contributions.native_boundaries != source.contributions.native_boundaries
}

fn limited_string_list(values: &[String]) -> String {
    const LIMIT: usize = 8;
    if values.is_empty() {
        return "-".to_string();
    }
    let mut selected = values.iter().take(LIMIT).cloned().collect::<Vec<_>>();
    if values.len() > LIMIT {
        selected.push(format!("...+{}", values.len() - LIMIT));
    }
    selected.join(",")
}

fn describe_runtime_contract_mismatch(
    installed: &PlatformPackageManifest,
    source: &PlatformPackageManifest,
) -> String {
    let mut parts = Vec::new();
    if installed.api_version != source.api_version {
        parts.push(format!(
            "apiVersion installed={} source={}",
            installed.api_version, source.api_version
        ));
    }
    if installed.install_kind != source.install_kind {
        parts.push(format!(
            "installKind installed={} source={}",
            installed.install_kind, source.install_kind
        ));
    }
    if installed.ui != source.ui {
        parts.push("ui differs".to_string());
    }
    if installed.capabilities != source.capabilities {
        parts.push(format!(
            "capabilities installed={} source={}",
            installed.capabilities.len(),
            source.capabilities.len()
        ));
    }
    if installed.contributions.native_boundaries != source.contributions.native_boundaries {
        parts.push(format!(
            "nativeBoundaries installed={} source={}",
            installed.contributions.native_boundaries.len(),
            source.contributions.native_boundaries.len()
        ));
    }
    if installed.adapter != source.adapter {
        let installed_methods = installed
            .adapter
            .as_ref()
            .map(|adapter| adapter.methods.as_slice())
            .unwrap_or(&[]);
        let source_methods = source
            .adapter
            .as_ref()
            .map(|adapter| adapter.methods.as_slice())
            .unwrap_or(&[]);
        let installed_only = installed_methods
            .iter()
            .filter(|method| !source_methods.contains(method))
            .cloned()
            .collect::<Vec<_>>();
        let source_only = source_methods
            .iter()
            .filter(|method| !installed_methods.contains(method))
            .cloned()
            .collect::<Vec<_>>();
        parts.push(format!(
            "adapter differs installedMethods={} sourceMethods={} installedOnly=[{}] sourceOnly=[{}]",
            installed_methods.len(),
            source_methods.len(),
            limited_string_list(&installed_only),
            limited_string_list(&source_only)
        ));
    }

    if parts.is_empty() {
        "unknown runtime contract mismatch".to_string()
    } else {
        parts.join("; ")
    }
}

fn same_version_runtime_contract_mismatch(
    installed: &PlatformPackageManifest,
    source: &PlatformPackageManifest,
) -> bool {
    installed.version == source.version && runtime_contract_mismatch(installed, source)
}

fn same_version_local_package_content_mismatch(
    platform_id: &str,
    installed: &PlatformPackageManifest,
    source: &PlatformPackageManifest,
    installed_root: &Path,
    source_root: &Path,
) -> Result<bool, String> {
    if !strict_local_source_validation_enabled() || installed.version != source.version {
        return Ok(false);
    }

    let installed_fingerprint = package_dir_fingerprint(installed_root)?;
    let source_fingerprint = package_dir_fingerprint(source_root)?;
    if installed_fingerprint == source_fingerprint {
        return Ok(false);
    }

    logger::log_warn(&format!(
        "[PlatformPackage] 本地平台包内容不一致: platform={}, version={}, installedHash={}, sourceHash={}",
        platform_id, installed.version, installed_fingerprint, source_fingerprint
    ));
    Ok(true)
}

fn same_version_local_package_content_error(
    platform_id: &str,
    installed: &PlatformPackageManifest,
    source: &PlatformPackageManifest,
    installed_root: &Path,
    source_root: &Path,
) -> Option<String> {
    match same_version_local_package_content_mismatch(
        platform_id,
        installed,
        source,
        installed_root,
        source_root,
    ) {
        Ok(true) => Some("已安装平台包与当前本地包内容不一致，请修复或重新安装平台包".to_string()),
        Ok(false) => None,
        Err(error) => {
            logger::log_warn(&format!(
                "[PlatformPackage] 本地平台包内容校验失败: platform={}, error={}",
                platform_id, error
            ));
            Some(format!("本地平台包内容校验失败: {}", error))
        }
    }
}

fn read_installed_manifest_and_update_state(
    platform_id: &str,
) -> Result<(Option<PlatformPackageManifest>, Option<String>), String> {
    let current_dir = package_current_dir(platform_id)?;
    if !current_dir.join(MANIFEST_FILE).exists() {
        return Ok((None, None));
    }

    match validate_manifest(platform_id, &current_dir) {
        Ok(manifest) => {
            let mut registry = read_registry()?;
            upsert_record(
                &mut registry,
                PersistedPlatformPackage {
                    platform_id: platform_id.to_string(),
                    installed: true,
                    runtime_ready: true,
                    installed_version: Some(manifest.version.clone()),
                    last_checked_at: Some(now_ts_ms()),
                    error_message: None,
                },
            );
            write_registry(&registry)?;
            Ok((Some(manifest), None))
        }
        Err(error) => {
            let mut registry = read_registry()?;
            let installed_version =
                get_record(&registry, platform_id).and_then(|item| item.installed_version.clone());
            upsert_record(
                &mut registry,
                PersistedPlatformPackage {
                    platform_id: platform_id.to_string(),
                    installed: true,
                    runtime_ready: false,
                    installed_version,
                    last_checked_at: Some(now_ts_ms()),
                    error_message: Some(error.clone()),
                },
            );
            write_registry(&registry)?;
            Ok((None, Some(error)))
        }
    }
}

pub fn list_platform_packages(app: &AppHandle) -> Result<Vec<PlatformPackageState>, String> {
    let registry = read_registry()?;
    let mut states = Vec::new();
    for platform_id in SUPPORTED_PLATFORM_IDS {
        let (installed_manifest, validation_error) =
            read_installed_manifest_and_update_state(platform_id)?;
        let refreshed_registry = read_registry()?;
        let (source_manifest, source_root) =
            read_latest_source_manifest_and_root(app, platform_id, false);
        states.push(build_state(
            platform_id,
            get_record(&refreshed_registry, platform_id)
                .or_else(|| get_record(&registry, platform_id)),
            installed_manifest,
            source_manifest,
            source_root,
            validation_error,
        )?);
    }
    Ok(states)
}

pub fn check_platform_package_update(
    app: &AppHandle,
    platform_id: &str,
) -> Result<PlatformPackageState, String> {
    ensure_supported_platform(platform_id)?;
    logger::log_info(&format!(
        "[PlatformPackage] 强制检查平台包更新: {}",
        platform_id
    ));

    let (installed_manifest, validation_error) =
        read_installed_manifest_and_update_state(platform_id)?;
    let (source_manifest, source_root) =
        read_latest_source_manifest_and_root(app, platform_id, true);
    let mut registry = read_registry()?;
    let existing = get_record(&registry, platform_id).cloned();
    let installed_version = installed_manifest
        .as_ref()
        .map(|manifest| manifest.version.clone())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|item| item.installed_version.clone())
        });
    let error_message = validation_error.clone().or_else(|| {
        existing
            .as_ref()
            .and_then(|item| item.error_message.clone())
    });
    let installed = installed_manifest.is_some()
        || existing
            .as_ref()
            .map(|item| item.installed)
            .unwrap_or(false);
    let runtime_ready = installed_manifest.is_some()
        && validation_error.is_none()
        && existing
            .as_ref()
            .map(|item| item.runtime_ready)
            .unwrap_or(false);

    upsert_record(
        &mut registry,
        PersistedPlatformPackage {
            platform_id: platform_id.to_string(),
            installed,
            runtime_ready,
            installed_version,
            last_checked_at: Some(now_ts_ms()),
            error_message,
        },
    );
    write_registry(&registry)?;
    let refreshed_registry = read_registry()?;

    build_state(
        platform_id,
        get_record(&refreshed_registry, platform_id),
        installed_manifest,
        source_manifest,
        source_root,
        validation_error,
    )
}

pub fn install_platform_package(
    app: &AppHandle,
    platform_id: &str,
) -> Result<PlatformPackageState, String> {
    ensure_supported_platform(platform_id)?;
    logger::log_info(&format!(
        "[PlatformPackage] 安装平台包开始: {}",
        platform_id
    ));
    crate::modules::platform_adapter::stop_platform_adapter(platform_id);

    let source = resolve_package_source(app, platform_id, true)?;
    let source_manifest = source.manifest().clone();
    let source_root = match &source {
        PlatformPackageSource::Local { dir, .. } => Some(dir.clone()),
        PlatformPackageSource::Remote { .. } => None,
    };

    let installed_manifest = match match &source {
        PlatformPackageSource::Local { dir, .. } => install_local_source(platform_id, dir),
        PlatformPackageSource::Remote { package, .. } => {
            install_remote_source(platform_id, package)
        }
    } {
        Ok(manifest) => manifest,
        Err(error) => {
            let mut registry = read_registry()?;
            upsert_record(
                &mut registry,
                PersistedPlatformPackage {
                    platform_id: platform_id.to_string(),
                    installed: true,
                    runtime_ready: false,
                    installed_version: None,
                    last_checked_at: Some(now_ts_ms()),
                    error_message: Some(error.clone()),
                },
            );
            write_registry(&registry)?;
            return Err(error);
        }
    };

    let mut registry = read_registry()?;
    upsert_record(
        &mut registry,
        PersistedPlatformPackage {
            platform_id: platform_id.to_string(),
            installed: true,
            runtime_ready: true,
            installed_version: Some(installed_manifest.version.clone()),
            last_checked_at: Some(now_ts_ms()),
            error_message: None,
        },
    );
    write_registry(&registry)?;
    logger::log_info(&format!(
        "[PlatformPackage] 安装平台包完成: {}",
        platform_id
    ));

    build_state(
        platform_id,
        get_record(&registry, platform_id),
        Some(installed_manifest),
        Some(source_manifest),
        source_root,
        None,
    )
}

pub fn update_platform_package(
    app: &AppHandle,
    platform_id: &str,
) -> Result<PlatformPackageState, String> {
    logger::log_info(&format!(
        "[PlatformPackage] 更新平台包开始: {}",
        platform_id
    ));
    install_platform_package(app, platform_id)
}

pub fn uninstall_platform_package(
    app: Option<&AppHandle>,
    platform_id: &str,
) -> Result<PlatformPackageState, String> {
    ensure_supported_platform(platform_id)?;
    logger::log_info(&format!(
        "[PlatformPackage] 卸载平台包开始: {}",
        platform_id
    ));
    if platform_id == ANTIGRAVITY_PLATFORM_ID {
        crate::modules::platform_adapter::stop_antigravity_runtime_before_uninstall();
    } else if platform_id == ANTIGRAVITY_IDE_PLATFORM_ID {
        crate::modules::platform_adapter::stop_antigravity_ide_runtime_before_uninstall();
    } else if platform_id == CLAUDE_MANAGER_PLATFORM_ID {
        crate::modules::platform_adapter::stop_claude_manager_runtime_before_uninstall();
    } else if platform_id == ZED_PLATFORM_ID {
        crate::modules::platform_adapter::stop_zed_runtime_before_uninstall();
    } else if platform_id == KIRO_PLATFORM_ID {
        crate::modules::platform_adapter::stop_kiro_runtime_before_uninstall();
    } else if platform_id == GITHUB_COPILOT_PLATFORM_ID {
        crate::modules::platform_adapter::stop_github_copilot_runtime_before_uninstall();
    } else if platform_id == WINDSURF_PLATFORM_ID {
        crate::modules::platform_adapter::stop_windsurf_runtime_before_uninstall();
    } else if platform_id == CURSOR_PLATFORM_ID {
        crate::modules::platform_adapter::stop_cursor_runtime_before_uninstall();
    } else if platform_id == GEMINI_PLATFORM_ID {
        crate::modules::platform_adapter::stop_gemini_runtime_before_uninstall();
    } else if platform_id == TRAE_PLATFORM_ID {
        crate::modules::platform_adapter::stop_trae_runtime_before_uninstall();
    } else if platform_id == QODER_PLATFORM_ID {
        crate::modules::platform_adapter::stop_qoder_runtime_before_uninstall();
    } else if platform_id == CODEBUDDY_PLATFORM_ID {
        crate::modules::platform_adapter::stop_codebuddy_runtime_before_uninstall();
    } else if platform_id == CODEBUDDY_CN_PLATFORM_ID {
        crate::modules::platform_adapter::stop_codebuddy_cn_runtime_before_uninstall();
    } else if platform_id == WORKBUDDY_PLATFORM_ID {
        crate::modules::platform_adapter::stop_workbuddy_runtime_before_uninstall();
    } else if platform_id == CODEX_PLATFORM_ID {
        crate::modules::platform_adapter::stop_codex_runtime_before_uninstall();
    }

    let source_manifest = app.and_then(|app| read_latest_source_manifest(app, platform_id, false));
    let installed_manifest = read_installed_manifest(platform_id).ok().flatten();
    let platform_dir = package_dir(platform_id)?;
    if platform_dir.exists() {
        fs::remove_dir_all(&platform_dir).map_err(|err| {
            format!(
                "删除平台包目录失败: path={}, error={}",
                platform_dir.display(),
                err
            )
        })?;
    }

    let mut registry = read_registry()?;
    upsert_record(
        &mut registry,
        PersistedPlatformPackage {
            platform_id: platform_id.to_string(),
            installed: false,
            runtime_ready: false,
            installed_version: None,
            last_checked_at: Some(now_ts_ms()),
            error_message: None,
        },
    );
    write_registry(&registry)?;
    logger::log_info(&format!(
        "[PlatformPackage] 卸载平台包完成: {}",
        platform_id
    ));

    build_state(
        platform_id,
        get_record(&registry, platform_id),
        None,
        source_manifest.or(installed_manifest),
        None,
        None,
    )
}

pub fn is_platform_package_runtime_ready(platform_id: &str) -> bool {
    let Ok(registry) = read_registry() else {
        return false;
    };
    let Some(record) = get_record(&registry, platform_id) else {
        return false;
    };
    if !record.installed || !record.runtime_ready {
        return false;
    }
    let Ok(current_dir) = package_current_dir(platform_id) else {
        return false;
    };
    let Ok(installed_manifest) = validate_manifest(platform_id, &current_dir) else {
        return false;
    };
    if let Some(source_root) = local_source_package_dir_from_current_process(platform_id) {
        if let Ok(source_manifest) = validate_manifest(platform_id, &source_root) {
            if same_version_local_package_content_mismatch(
                platform_id,
                &installed_manifest,
                &source_manifest,
                &current_dir,
                &source_root,
            )
            .unwrap_or(true)
            {
                return false;
            }
        }
    }
    true
}

pub fn is_platform_package_installed(platform_id: &str) -> bool {
    is_platform_package_runtime_ready(platform_id)
}

pub fn ensure_platform_package_installed(platform_id: &str) -> Result<(), String> {
    if is_platform_package_runtime_ready(platform_id) {
        return Ok(());
    }
    Err(format!(
        "平台包未安装或未就绪，请先在平台管理中安装/修复: {}",
        platform_id
    ))
}

pub fn get_platform_package_ui_entry(platform_id: &str) -> Result<PlatformPackageUiEntry, String> {
    ensure_platform_package_installed(platform_id)?;
    let current_dir = package_current_dir(platform_id)?;
    let manifest = validate_manifest(platform_id, &current_dir)?;
    let ui = manifest
        .ui
        .clone()
        .ok_or_else(|| format!("平台包未声明 UI runtime: {}", platform_id))?;
    let entry_path = safe_relative_path(&ui.entry, "平台包 UI entry")?;
    let ui_path = current_dir.join(entry_path);
    let source = fs::read_to_string(&ui_path).map_err(|err| {
        format!(
            "读取平台包 UI 失败: path={}, error={}",
            ui_path.display(),
            err
        )
    })?;
    let style = match ui.style.as_deref() {
        Some(style_entry) => {
            let style_path = safe_relative_path(style_entry, "平台包 UI style")?;
            let style_path = current_dir.join(style_path);
            Some(fs::read_to_string(&style_path).map_err(|err| {
                format!(
                    "读取平台包 UI style 失败: path={}, error={}",
                    style_path.display(),
                    err
                )
            })?)
        }
        None => None,
    };

    Ok(PlatformPackageUiEntry {
        platform_id: platform_id.to_string(),
        version: manifest.version,
        protocol: ui.protocol,
        entry: ui.entry,
        exports: ui.exports,
        sandbox: ui.sandbox,
        source,
        style,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest(version: &str) -> PlatformPackageManifest {
        PlatformPackageManifest {
            id: ZED_PLATFORM_ID.to_string(),
            platform_id: ZED_PLATFORM_ID.to_string(),
            version: version.to_string(),
            api_version: PLATFORM_PACKAGE_API_VERSION,
            min_core_version: "0.0.0".to_string(),
            display_name: "Zed".to_string(),
            entry: "runtime/index.json".to_string(),
            package_mode: "hotUpdate".to_string(),
            install_kind: "coreNativeBoundary".to_string(),
            adapter: None,
            ui: None,
            capabilities: vec!["accounts".to_string()],
            contributions: PlatformPackageContributions::default(),
            changelog: Vec::new(),
            download_size_bytes: None,
            sha256: None,
            signature: None,
        }
    }

    fn test_remote_package(version: &str) -> PlatformPackageRemotePackage {
        PlatformPackageRemotePackage {
            id: ZED_PLATFORM_ID.to_string(),
            platform_id: ZED_PLATFORM_ID.to_string(),
            version: version.to_string(),
            api_version: PLATFORM_PACKAGE_API_VERSION,
            min_core_version: "0.0.0".to_string(),
            display_name: "Zed".to_string(),
            entry: "runtime/index.json".to_string(),
            package_mode: "hotUpdate".to_string(),
            install_kind: "coreNativeBoundary".to_string(),
            adapter: None,
            ui: None,
            capabilities: vec!["accounts".to_string()],
            contributions: PlatformPackageContributions::default(),
            changelog: Vec::new(),
            artifacts: Vec::new(),
            download_url: Some("https://example.com/zed.zip".to_string()),
            download_size_bytes: Some(1),
            sha256: Some("0".repeat(64)),
            signature: None,
        }
    }

    #[test]
    fn bundled_zed_source_manifest_matches_runtime() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source_dir = manifest_dir
            .parent()
            .expect("repo root")
            .join(PLATFORM_PACKAGE_DIR)
            .join(ZED_PLATFORM_ID);
        let manifest = validate_manifest(ZED_PLATFORM_ID, &source_dir).expect("valid zed package");
        assert_eq!(manifest.platform_id, ZED_PLATFORM_ID);
        assert_eq!(manifest.package_mode, "hotUpdate");
        assert!(manifest
            .contributions
            .platforms
            .iter()
            .any(|platform| platform.id == ZED_PLATFORM_ID && platform.page == "zed"));
    }

    #[test]
    fn bundled_kiro_source_manifest_matches_runtime() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source_dir = manifest_dir
            .parent()
            .expect("repo root")
            .join(PLATFORM_PACKAGE_DIR)
            .join(KIRO_PLATFORM_ID);
        let manifest =
            validate_manifest(KIRO_PLATFORM_ID, &source_dir).expect("valid kiro package");
        assert_eq!(manifest.platform_id, KIRO_PLATFORM_ID);
        assert_eq!(manifest.package_mode, "hotUpdate");
        assert_eq!(manifest.install_kind, "sidecarAdapter");
        assert!(manifest
            .contributions
            .platforms
            .iter()
            .any(|platform| platform.id == KIRO_PLATFORM_ID && platform.page == "kiro"));
    }

    #[test]
    fn rejects_unsafe_runtime_entry_path() {
        assert!(safe_relative_path("../runtime/index.json", "entry").is_err());
        assert!(safe_relative_path("/runtime/index.json", "entry").is_err());
        assert!(safe_relative_path("runtime/index.json", "entry").is_ok());
    }

    #[test]
    fn prefers_local_source_when_remote_version_matches() {
        let remote = PlatformPackageSource::Remote {
            package: test_remote_package("1.0.0"),
            manifest: test_manifest("1.0.0"),
        };
        let local = PlatformPackageSource::Local {
            dir: PathBuf::from("/tmp/zed-local"),
            manifest: test_manifest("1.0.0"),
        };

        let picked = pick_latest_source(Some(remote), Some(local)).expect("source");
        assert!(matches!(picked, PlatformPackageSource::Local { .. }));
    }

    #[test]
    fn prefers_remote_source_when_remote_version_is_newer() {
        let remote = PlatformPackageSource::Remote {
            package: test_remote_package("1.1.0"),
            manifest: test_manifest("1.1.0"),
        };
        let local = PlatformPackageSource::Local {
            dir: PathBuf::from("/tmp/zed-local"),
            manifest: test_manifest("1.0.0"),
        };

        let picked = pick_latest_source(Some(remote), Some(local)).expect("source");
        assert!(matches!(picked, PlatformPackageSource::Remote { .. }));
    }
}
