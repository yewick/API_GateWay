//! 客户端接入：把外部 AI 客户端（首版 Codex）的本地配置以增量、可回滚的方式接入 YeAPI。
//!
//! 这里只处理“本地配置文件维护”，不是 HTTP 微服务（见 `services/`）。

pub mod codex;
pub mod paths;
pub mod state;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Codex,
}

impl ClientKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim() {
            "codex" => Ok(Self::Codex),
            other => Err(format!("暂不支持的客户端: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
        }
    }
}

/// 一个客户端的用户级配置目录与关键文件。
pub struct ClientPaths {
    pub home: PathBuf,
    pub config: PathBuf,
    pub auth: PathBuf,
}

/// 串行化 apply / disconnect，避免两个入口同时改同一份配置。
pub static SETUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Serialize)]
pub struct ClientSetupStatus {
    pub client: String,
    pub display_name: String,
    pub installed: bool,
    pub applied: bool,
    pub drift: bool,
    pub config_parse_error: Option<String>,
    pub codex_home: String,
    pub config_path: String,
    pub auth_path: String,
    pub auth_logged_in: bool,
    pub requires_openai_auth: bool,
    pub current_model_provider: Option<String>,
    pub current_model: Option<String>,
    pub yeapi_provider_present: bool,
    pub provider_token_hint: Option<String>,
    pub applied_api_key_id: Option<String>,
    pub applied_api_key_hint: Option<String>,
    pub applied_api_key_valid: bool,
    pub applied_at: Option<String>,
    pub applied_base_url: Option<String>,
    pub gateway_base_url: String,
    pub base_url_matches: bool,
    pub backup_path: Option<String>,
}

/// Key 来源：新建专用 Key，或复用已有 Key。明文 Key 不经过 IPC。
#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ApiKeySelection {
    New {
        name: String,
        #[serde(default)]
        allowed_models: Vec<String>,
        quota_limit: Option<i64>,
        expires_at: Option<String>,
    },
    Existing {
        id: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct ApplyClientInput {
    pub client: String,
    pub key: ApiKeySelection,
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyClientResult {
    pub status: ClientSetupStatus,
    pub backup_path: Option<String>,
    pub restart_required: bool,
}

#[derive(Debug, Serialize)]
pub struct DisconnectClientResult {
    pub status: ClientSetupStatus,
    pub backup_path: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClientTestResult {
    pub ok: bool,
    pub status: u16,
    pub latency_ms: u64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ModelOption {
    pub id: String,
    pub owned_by: String,
}

#[derive(Debug, Serialize)]
pub struct PreviewResult {
    pub config_path: String,
    pub snippet: String,
}

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {e}"))
}

/// Codex 的 base_url 必须止于 `/v1`（客户端自己拼 `/responses`）。
pub(crate) async fn gateway_base_url(app: &tauri::AppHandle, state: &crate::AppState) -> String {
    let actual = *state.server_port.read().await;
    let port = if actual == 0 {
        crate::server::get_server_port(app)
    } else {
        actual
    };
    let host = crate::server::get_server_host(app);
    let host = match host.trim() {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
        other if other.is_empty() => "127.0.0.1".to_string(),
        other => other.to_string(),
    };
    format!("http://{host}:{port}/v1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_input_deserializes_new_key_payload() {
        let payload = serde_json::json!({
            "client": "codex",
            "key": {
                "mode": "new",
                "name": "Codex",
                "allowed_models": [],
                "quota_limit": -1
            },
            "model": "deepseek-v4-pro"
        });
        let input: ApplyClientInput = serde_json::from_value(payload).unwrap();
        assert_eq!(input.client, "codex");
        assert_eq!(input.model, "deepseek-v4-pro");
        match input.key {
            ApiKeySelection::New {
                name, quota_limit, ..
            } => {
                assert_eq!(name, "Codex");
                assert_eq!(quota_limit, Some(-1));
            }
            _ => panic!("expected new key selection"),
        }
    }

    #[test]
    fn apply_input_deserializes_existing_key_payload() {
        let payload = serde_json::json!({
            "client": "codex",
            "key": { "mode": "existing", "id": "key-123" },
            "model": "m"
        });
        let input: ApplyClientInput = serde_json::from_value(payload).unwrap();
        match input.key {
            ApiKeySelection::Existing { id } => assert_eq!(id, "key-123"),
            _ => panic!("expected existing key selection"),
        }
    }
}
