//! 共享鉴权工具：`Bearer`/`x-api-key` 提取 + 密钥查找 + 过期检查。
//!
//! 供 `/v1/messages`、`/v1/responses`、`/api/kb/ask` 等端点复用，避免重复实现。

use axum::http::HeaderMap;

use crate::db::models::ApiKey;
use crate::db::repository::Repository;
use crate::protocol;

/// 判断密钥是否已过期（`expires_at` 为空视为永不过期）。
pub fn key_is_expired(expires_at: &str) -> bool {
    if expires_at.is_empty() {
        return false;
    }
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expiry| chrono::Utc::now() > expiry)
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%dT%H:%M:%S%.3fZ")
                .map(|d| chrono::Utc::now() > d.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(expires_at, "%Y-%m-%d")
                .map(|d| chrono::Utc::now() > d.and_hms_opt(23, 59, 59).unwrap().and_utc())
        })
        .unwrap_or(false)
}

/// 从请求头鉴权并返回密钥记录：提取 bearer → 查库 → 过期检查。
/// 失败返回 `(状态码, 错误信息)`。
pub async fn authenticate(
    repo: &Repository,
    headers: &HeaderMap,
) -> Result<ApiKey, (u16, String)> {
    let api_key = match protocol::extract_api_key(headers) {
        Some(k) => k,
        None => return Err((401, "Missing API key".to_string())),
    };

    let key_record = match repo.get_api_key_by_key(&api_key).await {
        Ok(k) => k,
        Err(_) => return Err((401, "Invalid API key".to_string())),
    };

    if let Some(ref expires_at) = key_record.expires_at {
        if key_is_expired(expires_at) {
            return Err((401, "API key has expired".to_string()));
        }
    }

    Ok(key_record)
}
