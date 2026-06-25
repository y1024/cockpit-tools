use crate::models::Account;
use serde_json::json;

fn call<T: serde::de::DeserializeOwned>(
    method: &str,
    payload: serde_json::Value,
) -> Result<T, String> {
    crate::modules::platform_adapter::call_antigravity_series(method, payload)
}

pub fn list_accounts() -> Result<Vec<Account>, String> {
    call("accounts.list", json!({}))
}

pub fn load_account(account_id: &str) -> Result<Account, String> {
    list_accounts()?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| format!("找不到账号 ID: {}", account_id))
}

pub fn save_account(account: &Account) -> Result<Account, String> {
    call("accounts.saveSnapshot", json!({ "account": account }))
}

pub fn get_current_account_id() -> Result<Option<String>, String> {
    call("accounts.currentId", json!({}))
}
