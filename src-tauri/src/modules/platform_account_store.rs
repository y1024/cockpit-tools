use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
#[cfg(test)]
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::modules::logger;

const DB_FILE: &str = "account_store.sqlite";
const SCHEMA_VERSION: i64 = 1;
#[cfg(test)]
thread_local! {
    static TEST_DATABASE_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone)]
struct AccountRecordMetadata {
    email: Option<String>,
    display_name: Option<String>,
    plan_type: Option<String>,
    auth_mode: Option<String>,
    created_at: i64,
    last_used: i64,
}

pub fn database_path() -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        if let Some(dir) = TEST_DATABASE_DIR.with(|value| value.borrow().clone()) {
            return Ok(dir.join(DB_FILE));
        }
    }

    Ok(crate::modules::app_data::get_data_dir()?.join(DB_FILE))
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn read_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .and_then(|item| normalize_optional_string(Some(item)))
    })
}

fn read_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        let item = value.get(*key)?;
        item.as_i64()
            .or_else(|| item.as_u64().and_then(|raw| i64::try_from(raw).ok()))
            .or_else(|| item.as_str().and_then(|raw| raw.trim().parse::<i64>().ok()))
    })
}

fn metadata_from_value(value: &Value) -> AccountRecordMetadata {
    let created_at = read_i64(value, &["created_at", "createdAt"]).unwrap_or_else(now_timestamp);
    let last_used = read_i64(value, &["last_used", "lastUsed"]).unwrap_or(created_at);

    AccountRecordMetadata {
        email: read_string(
            value,
            &[
                "email",
                "github_email",
                "githubEmail",
                "github_login",
                "githubLogin",
                "user_id",
                "userId",
            ],
        ),
        display_name: read_string(
            value,
            &[
                "name",
                "nickname",
                "display_name",
                "displayName",
                "github_name",
                "githubName",
                "github_login",
                "githubLogin",
                "account_name",
                "accountName",
                "api_provider_name",
                "apiProviderName",
            ],
        ),
        plan_type: read_string(
            value,
            &[
                "plan_type",
                "planType",
                "plan_name",
                "planName",
                "plan_raw",
                "planRaw",
                "copilot_plan",
                "copilotPlan",
            ],
        ),
        auth_mode: read_string(value, &["auth_mode", "authMode"]),
        created_at,
        last_used,
    }
}

fn id_from_value_or_path(value: &Value, path: &Path) -> Option<String> {
    read_string(value, &["id"]).or_else(|| {
        path.file_stem()
            .and_then(|name| name.to_str())
            .and_then(|name| normalize_optional_string(Some(name)))
    })
}

fn open_connection() -> Result<Connection, String> {
    let path = database_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "创建账号数据库目录失败: path={}, error={}",
                parent.display(),
                error
            )
        })?;
    }

    let conn = Connection::open(&path).map_err(|error| {
        format!(
            "打开账号数据库失败: path={}, error={}",
            path.display(),
            error
        )
    })?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| format!("设置账号数据库 busy_timeout 失败: {}", error))?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| format!("启用账号数据库 WAL 失败: {}", error))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| format!("启用账号数据库 foreign_keys 失败: {}", error))?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS account_records (
            platform TEXT NOT NULL,
            id TEXT NOT NULL,
            account_json TEXT NOT NULL,
            email TEXT,
            display_name TEXT,
            plan_type TEXT,
            auth_mode TEXT,
            created_at INTEGER NOT NULL DEFAULT 0,
            last_used INTEGER NOT NULL DEFAULT 0,
            sort_order INTEGER,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (platform, id)
        );

        CREATE INDEX IF NOT EXISTS idx_account_records_platform_recency
            ON account_records(platform, sort_order ASC, last_used DESC, created_at DESC, id ASC);

        CREATE TABLE IF NOT EXISTS account_platform_state (
            platform TEXT PRIMARY KEY,
            current_account_id TEXT,
            index_json TEXT,
            source_index_path TEXT,
            migrated_from_json_at INTEGER,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS account_store_migrations (
            platform TEXT PRIMARY KEY,
            source_kind TEXT NOT NULL,
            source_index_path TEXT,
            source_accounts_dir TEXT,
            migrated_at INTEGER NOT NULL,
            account_count INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|error| format!("初始化账号数据库 schema 失败: {}", error))?;
    let _ = conn.execute(
        "ALTER TABLE account_records ADD COLUMN sort_order INTEGER",
        [],
    );
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|error| format!("写入账号数据库版本失败: {}", error))?;
    Ok(())
}

fn platform_is_migrated_tx(tx: &Transaction<'_>, platform: &str) -> Result<bool, String> {
    tx.query_row(
        "SELECT 1 FROM account_store_migrations WHERE platform = ?1 LIMIT 1",
        [platform],
        |_| Ok(()),
    )
    .optional()
    .map(|item| item.is_some())
    .map_err(|error| {
        format!(
            "读取账号迁移状态失败: platform={}, error={}",
            platform, error
        )
    })
}

fn upsert_account_value_tx(
    tx: &Transaction<'_>,
    platform: &str,
    id: &str,
    value: &Value,
    sort_order: Option<i64>,
) -> Result<(), String> {
    let metadata = metadata_from_value(value);
    let account_json =
        serde_json::to_string(value).map_err(|error| format!("序列化账号 JSON 失败: {}", error))?;
    tx.execute(
        r#"
        INSERT INTO account_records (
            platform, id, account_json, email, display_name, plan_type, auth_mode,
            created_at, last_used, sort_order, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(platform, id) DO UPDATE SET
            account_json = excluded.account_json,
            email = excluded.email,
            display_name = excluded.display_name,
            plan_type = excluded.plan_type,
            auth_mode = excluded.auth_mode,
            created_at = excluded.created_at,
            last_used = excluded.last_used,
            sort_order = COALESCE(excluded.sort_order, account_records.sort_order),
            updated_at = excluded.updated_at
        "#,
        params![
            platform,
            id,
            account_json,
            metadata.email,
            metadata.display_name,
            metadata.plan_type,
            metadata.auth_mode,
            metadata.created_at,
            metadata.last_used,
            sort_order,
            now_timestamp()
        ],
    )
    .map_err(|error| {
        format!(
            "写入账号数据库失败: platform={}, id={}, error={}",
            platform, id, error
        )
    })?;
    Ok(())
}

pub fn ensure_platform_migrated_from_json(
    platform: &str,
    index_path: &Path,
    accounts_dir: &Path,
) -> Result<(), String> {
    let mut conn = open_connection()?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("创建账号迁移事务失败: {}", error))?;
    if platform_is_migrated_tx(&tx, platform)? {
        return Ok(());
    }

    let index_value = fs::read_to_string(index_path)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok());
    let current_account_id = index_value
        .as_ref()
        .and_then(|value| read_string(value, &["current_account_id", "currentAccountId"]));
    let ordered_ids = index_value
        .as_ref()
        .and_then(|value| value.get("accounts"))
        .and_then(Value::as_array)
        .map(|accounts| {
            accounts
                .iter()
                .filter_map(|item| read_string(item, &["id"]))
                .enumerate()
                .map(|(index, id)| (id, i64::try_from(index).unwrap_or(i64::MAX)))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();

    let mut imported_ids = Vec::new();
    if accounts_dir.exists() {
        let entries = fs::read_dir(accounts_dir).map_err(|error| {
            format!(
                "扫描账号详情目录失败: platform={}, dir={}, error={}",
                platform,
                accounts_dir.display(),
                error
            )
        })?;

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    logger::log_warn(&format!(
                        "[AccountStore] 跳过无法读取的账号目录项: platform={}, dir={}, error={}",
                        platform,
                        accounts_dir.display(),
                        error
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|item| item.to_str())
                .map(|item| item.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            if !is_json {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|item| item.to_str())
                .unwrap_or_default();
            if file_name.ends_with(".bak") {
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(error) => {
                    logger::log_warn(&format!(
                        "[AccountStore] 跳过无法读取的账号详情: platform={}, path={}, error={}",
                        platform,
                        path.display(),
                        error
                    ));
                    continue;
                }
            };
            let value = match serde_json::from_str::<Value>(&content) {
                Ok(value) => value,
                Err(error) => {
                    logger::log_warn(&format!(
                        "[AccountStore] 跳过无法解析的账号详情: platform={}, path={}, error={}",
                        platform,
                        path.display(),
                        error
                    ));
                    continue;
                }
            };
            let Some(id) = id_from_value_or_path(&value, &path) else {
                logger::log_warn(&format!(
                    "[AccountStore] 跳过缺少 id 的账号详情: platform={}, path={}",
                    platform,
                    path.display()
                ));
                continue;
            };
            let sort_order = ordered_ids.get(&id).copied();
            upsert_account_value_tx(&tx, platform, &id, &value, sort_order)?;
            imported_ids.push(id);
        }
    }

    let current_account_id =
        current_account_id.filter(|id| imported_ids.iter().any(|item| item == id));
    let index_json = index_value
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());
    tx.execute(
        r#"
        INSERT INTO account_platform_state (
            platform, current_account_id, index_json, source_index_path,
            migrated_from_json_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(platform) DO UPDATE SET
            current_account_id = excluded.current_account_id,
            index_json = excluded.index_json,
            source_index_path = excluded.source_index_path,
            migrated_from_json_at = excluded.migrated_from_json_at,
            updated_at = excluded.updated_at
        "#,
        params![
            platform,
            current_account_id,
            index_json,
            index_path.to_string_lossy().to_string(),
            now_timestamp(),
            now_timestamp()
        ],
    )
    .map_err(|error| {
        format!(
            "写入账号平台状态失败: platform={}, error={}",
            platform, error
        )
    })?;
    tx.execute(
        r#"
        INSERT INTO account_store_migrations (
            platform, source_kind, source_index_path, source_accounts_dir,
            migrated_at, account_count
        ) VALUES (?1, 'json', ?2, ?3, ?4, ?5)
        "#,
        params![
            platform,
            index_path.to_string_lossy().to_string(),
            accounts_dir.to_string_lossy().to_string(),
            now_timestamp(),
            i64::try_from(imported_ids.len()).unwrap_or(0)
        ],
    )
    .map_err(|error| {
        format!(
            "写入账号迁移状态失败: platform={}, error={}",
            platform, error
        )
    })?;
    tx.commit().map_err(|error| {
        format!(
            "提交账号迁移事务失败: platform={}, error={}",
            platform, error
        )
    })?;

    logger::log_info(&format!(
        "[AccountStore] 已完成平台账号 JSON 到 SQLite 的首次迁移: platform={}, accounts={}, index_path={}, accounts_dir={}",
        platform,
        imported_ids.len(),
        index_path.display(),
        accounts_dir.display()
    ));
    Ok(())
}

pub fn save_account<T: Serialize>(platform: &str, id: &str, account: &T) -> Result<(), String> {
    let value =
        serde_json::to_value(account).map_err(|error| format!("序列化账号数据失败: {}", error))?;
    let mut conn = open_connection()?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("创建账号保存事务失败: {}", error))?;
    upsert_account_value_tx(&tx, platform, id, &value, None)?;
    tx.commit().map_err(|error| {
        format!(
            "提交账号保存事务失败: platform={}, id={}, error={}",
            platform, id, error
        )
    })
}

pub fn load_account<T: DeserializeOwned>(
    platform: &str,
    account_id: &str,
) -> Result<Option<T>, String> {
    let conn = open_connection()?;
    let account_json = conn
        .query_row(
            "SELECT account_json FROM account_records WHERE platform = ?1 AND id = ?2",
            params![platform, account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            format!(
                "读取账号数据库失败: platform={}, id={}, error={}",
                platform, account_id, error
            )
        })?;
    account_json
        .map(|content| {
            serde_json::from_str::<T>(&content).map_err(|error| {
                format!(
                    "解析账号数据库记录失败: platform={}, id={}, error={}",
                    platform, account_id, error
                )
            })
        })
        .transpose()
}

pub fn list_accounts<T: DeserializeOwned>(platform: &str) -> Result<Vec<T>, String> {
    let conn = open_connection()?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT account_json
            FROM account_records
            WHERE platform = ?1
            ORDER BY sort_order IS NULL, sort_order ASC, last_used DESC, created_at DESC, id ASC
            "#,
        )
        .map_err(|error| {
            format!(
                "准备账号列表查询失败: platform={}, error={}",
                platform, error
            )
        })?;
    let rows = stmt
        .query_map([platform], |row| row.get::<_, String>(0))
        .map_err(|error| format!("查询账号列表失败: platform={}, error={}", platform, error))?;

    let mut accounts = Vec::new();
    for row in rows {
        let content = row.map_err(|error| {
            format!("读取账号列表行失败: platform={}, error={}", platform, error)
        })?;
        let account = serde_json::from_str::<T>(&content).map_err(|error| {
            format!("解析账号列表行失败: platform={}, error={}", platform, error)
        })?;
        accounts.push(account);
    }
    Ok(accounts)
}

pub fn save_account_order(platform: &str, ordered_ids: &[String]) -> Result<(), String> {
    let mut conn = open_connection()?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("创建账号排序事务失败: {}", error))?;
    for (index, id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE account_records SET sort_order = ?1, updated_at = ?2 WHERE platform = ?3 AND id = ?4",
            params![i64::try_from(index).unwrap_or(i64::MAX), now_timestamp(), platform, id],
        )
        .map_err(|error| {
            format!(
                "写入账号排序失败: platform={}, id={}, error={}",
                platform, id, error
            )
        })?;
    }
    tx.commit().map_err(|error| {
        format!(
            "提交账号排序事务失败: platform={}, error={}",
            platform, error
        )
    })
}

pub fn delete_account(platform: &str, account_id: &str) -> Result<(), String> {
    let conn = open_connection()?;
    conn.execute(
        "DELETE FROM account_records WHERE platform = ?1 AND id = ?2",
        params![platform, account_id],
    )
    .map_err(|error| {
        format!(
            "删除账号数据库记录失败: platform={}, id={}, error={}",
            platform, account_id, error
        )
    })?;
    let current = get_current_account_id(platform)?;
    if current.as_deref() == Some(account_id) {
        set_current_account_id(platform, None)?;
    }
    Ok(())
}

pub fn set_current_account_id(platform: &str, account_id: Option<&str>) -> Result<(), String> {
    let conn = open_connection()?;
    conn.execute(
        r#"
        INSERT INTO account_platform_state (platform, current_account_id, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(platform) DO UPDATE SET
            current_account_id = excluded.current_account_id,
            updated_at = excluded.updated_at
        "#,
        params![platform, account_id, now_timestamp()],
    )
    .map_err(|error| {
        format!(
            "写入当前账号状态失败: platform={}, account_id={:?}, error={}",
            platform, account_id, error
        )
    })?;
    Ok(())
}

pub fn get_current_account_id(platform: &str) -> Result<Option<String>, String> {
    let conn = open_connection()?;
    conn.query_row(
        "SELECT current_account_id FROM account_platform_state WHERE platform = ?1",
        [platform],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|item| item.flatten())
    .map_err(|error| {
        format!(
            "读取当前账号状态失败: platform={}, error={}",
            platform, error
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestAccount {
        id: String,
        email: String,
        created_at: i64,
        last_used: i64,
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    struct TestDbGuard {
        data_dir: PathBuf,
    }

    impl TestDbGuard {
        fn new(prefix: &str) -> Self {
            let data_dir = make_temp_dir(prefix);
            TEST_DATABASE_DIR.with(|value| {
                *value.borrow_mut() = Some(data_dir.clone());
            });
            Self { data_dir }
        }
    }

    impl Drop for TestDbGuard {
        fn drop(&mut self) {
            TEST_DATABASE_DIR.with(|value| {
                *value.borrow_mut() = None;
            });
            let _ = fs::remove_dir_all(&self.data_dir);
        }
    }

    #[test]
    fn migrates_platform_json_accounts_once() {
        let db = TestDbGuard::new("account-store-migration");
        let data_dir = &db.data_dir;

        let accounts_dir = data_dir.join("demo_accounts");
        fs::create_dir_all(&accounts_dir).expect("create accounts dir");
        let account = TestAccount {
            id: "demo-1".to_string(),
            email: "demo@example.com".to_string(),
            created_at: 10,
            last_used: 20,
        };
        fs::write(
            accounts_dir.join("demo-1.json"),
            serde_json::to_string(&account).expect("serialize account"),
        )
        .expect("write account");
        let index_path = data_dir.join("demo_accounts.json");
        fs::write(
            &index_path,
            r#"{"version":"1.0","current_account_id":"demo-1","accounts":[]}"#,
        )
        .expect("write index");

        ensure_platform_migrated_from_json("demo", &index_path, &accounts_dir)
            .expect("migrate platform");
        let loaded: TestAccount = load_account("demo", "demo-1")
            .expect("load account")
            .expect("account exists");
        assert_eq!(loaded, account);
        assert_eq!(
            get_current_account_id("demo").expect("current id"),
            Some("demo-1".to_string())
        );

        fs::write(
            accounts_dir.join("demo-2.json"),
            r#"{"id":"demo-2","email":"demo2@example.com","created_at":30,"last_used":40}"#,
        )
        .expect("write second account");
        ensure_platform_migrated_from_json("demo", &index_path, &accounts_dir)
            .expect("second migrate noop");
        assert!(load_account::<TestAccount>("demo", "demo-2")
            .expect("load second")
            .is_none());
    }

    #[test]
    fn migration_preserves_index_order() {
        let db = TestDbGuard::new("account-store-order-migration");
        let data_dir = &db.data_dir;

        let accounts_dir = data_dir.join("demo_accounts");
        fs::create_dir_all(&accounts_dir).expect("create accounts dir");
        for account in [
            TestAccount {
                id: "demo-a".to_string(),
                email: "a@example.com".to_string(),
                created_at: 10,
                last_used: 10,
            },
            TestAccount {
                id: "demo-b".to_string(),
                email: "b@example.com".to_string(),
                created_at: 20,
                last_used: 20,
            },
        ] {
            fs::write(
                accounts_dir.join(format!("{}.json", account.id)),
                serde_json::to_string(&account).expect("serialize account"),
            )
            .expect("write account");
        }
        let index_path = data_dir.join("demo_accounts.json");
        fs::write(
            &index_path,
            r#"{"version":"1.0","accounts":[{"id":"demo-a"},{"id":"demo-b"}]}"#,
        )
        .expect("write index");

        ensure_platform_migrated_from_json("demo-order-migrate", &index_path, &accounts_dir)
            .expect("migrate platform");
        let listed =
            list_accounts::<TestAccount>("demo-order-migrate").expect("list migrated accounts");
        let ids = listed
            .into_iter()
            .map(|account| account.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["demo-a".to_string(), "demo-b".to_string()]);
    }

    #[test]
    fn save_account_order_controls_list_order() {
        let _db = TestDbGuard::new("account-store-save-order");

        let account_a = TestAccount {
            id: "demo-a".to_string(),
            email: "a@example.com".to_string(),
            created_at: 10,
            last_used: 10,
        };
        let account_b = TestAccount {
            id: "demo-b".to_string(),
            email: "b@example.com".to_string(),
            created_at: 20,
            last_used: 20,
        };
        save_account("demo-save-order", &account_a.id, &account_a).expect("save account a");
        save_account("demo-save-order", &account_b.id, &account_b).expect("save account b");

        save_account_order(
            "demo-save-order",
            &["demo-a".to_string(), "demo-b".to_string()],
        )
        .expect("save account order");
        let listed = list_accounts::<TestAccount>("demo-save-order").expect("list accounts");
        let ids = listed
            .into_iter()
            .map(|account| account.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["demo-a".to_string(), "demo-b".to_string()]);
    }

    #[test]
    fn delete_clears_current_account() {
        let _db = TestDbGuard::new("account-store-delete");

        let account = TestAccount {
            id: "demo-1".to_string(),
            email: "demo@example.com".to_string(),
            created_at: 10,
            last_used: 20,
        };
        save_account("demo-delete", &account.id, &account).expect("save account");
        set_current_account_id("demo-delete", Some(&account.id)).expect("set current");
        delete_account("demo-delete", &account.id).expect("delete account");

        assert!(load_account::<TestAccount>("demo-delete", &account.id)
            .expect("load account")
            .is_none());
        assert_eq!(
            get_current_account_id("demo-delete").expect("current id"),
            None
        );
    }
}
