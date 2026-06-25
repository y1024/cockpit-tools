use std::fs;
use std::path::PathBuf;

// Keep the historical directory name for compatibility with existing installs.
const DATA_DIR: &str = ".antigravity_cockpit";
const DEV_DATA_DIR: &str = ".antigravity_cockpit_dev";
const TEST_DATA_DIR: &str = ".antigravity_cockpit_test";
const DATA_DIR_ENV: &str = "COCKPIT_TOOLS_DATA_DIR";
const PROFILE_ENV: &str = "COCKPIT_TOOLS_PROFILE";

pub fn profile_name() -> String {
    std::env::var(PROFILE_ENV)
        .ok()
        .or_else(|| option_env!("COCKPIT_TOOLS_PROFILE").map(ToString::to_string))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "prod".to_string())
}

pub fn is_dev_profile() -> bool {
    profile_name() == "dev"
}

pub fn is_test_profile() -> bool {
    profile_name() == "test"
}

pub fn resolve_data_dir() -> Result<PathBuf, String> {
    if let Ok(raw) = std::env::var(DATA_DIR_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    let profile = profile_name();
    let dir_name = match profile.as_str() {
        "dev" => DEV_DATA_DIR,
        "test" => TEST_DATA_DIR,
        _ => DATA_DIR,
    };
    Ok(home.join(dir_name))
}

pub fn resolve_instances_dir(name: &str) -> Result<PathBuf, String> {
    Ok(resolve_data_dir()?.join("instances").join(name))
}

pub fn get_data_dir() -> Result<PathBuf, String> {
    let data_dir = resolve_data_dir()?;

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    }

    Ok(data_dir)
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_data_dir, resolve_instances_dir, DATA_DIR, DATA_DIR_ENV, DEV_DATA_DIR,
        PROFILE_ENV, TEST_DATA_DIR,
    };
    use std::env;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct EnvGuard {
        previous_data_dir: Option<String>,
        previous_profile: Option<String>,
    }

    impl EnvGuard {
        fn set(data_dir: Option<&str>, profile: Option<&str>) -> Self {
            let guard = Self {
                previous_data_dir: env::var(DATA_DIR_ENV).ok(),
                previous_profile: env::var(PROFILE_ENV).ok(),
            };

            match data_dir {
                Some(value) => env::set_var(DATA_DIR_ENV, value),
                None => env::remove_var(DATA_DIR_ENV),
            }
            match profile {
                Some(value) => env::set_var(PROFILE_ENV, value),
                None => env::remove_var(PROFILE_ENV),
            }

            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous_data_dir.as_ref() {
                env::set_var(DATA_DIR_ENV, value);
            } else {
                env::remove_var(DATA_DIR_ENV);
            }

            if let Some(value) = self.previous_profile.as_ref() {
                env::set_var(PROFILE_ENV, value);
            } else {
                env::remove_var(PROFILE_ENV);
            }
        }
    }

    #[test]
    fn explicit_data_dir_env_wins_over_profile() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let custom_dir = "/tmp/cockpit-tools-custom-data-dir";
        let _guard = EnvGuard::set(Some(custom_dir), Some("dev"));

        assert_eq!(
            resolve_data_dir().expect("data dir should resolve"),
            PathBuf::from(custom_dir)
        );
    }

    #[test]
    fn dev_profile_uses_dev_data_dir() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _guard = EnvGuard::set(None, Some("dev"));

        assert_eq!(
            resolve_data_dir()
                .expect("data dir should resolve")
                .file_name()
                .and_then(|name| name.to_str()),
            Some(DEV_DATA_DIR)
        );
    }

    #[test]
    fn default_profile_uses_production_data_dir() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _guard = EnvGuard::set(None, None);

        assert_eq!(
            resolve_data_dir()
                .expect("data dir should resolve")
                .file_name()
                .and_then(|name| name.to_str()),
            Some(DATA_DIR)
        );
    }

    #[test]
    fn test_profile_uses_test_data_dir() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _guard = EnvGuard::set(None, Some("test"));

        assert_eq!(
            resolve_data_dir()
                .expect("data dir should resolve")
                .file_name()
                .and_then(|name| name.to_str()),
            Some(TEST_DATA_DIR)
        );
    }

    #[test]
    fn dev_profile_instances_dir_uses_dev_data_dir() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _guard = EnvGuard::set(None, Some("dev"));

        assert_eq!(
            resolve_instances_dir("kiro")
                .expect("instances dir should resolve")
                .components()
                .rev()
                .take(3)
                .filter_map(|component| component.as_os_str().to_str())
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec![
                "kiro".to_string(),
                "instances".to_string(),
                DEV_DATA_DIR.to_string()
            ]
        );
    }
}
