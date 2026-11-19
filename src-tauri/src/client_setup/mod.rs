//! 客户端接入：把外部 AI 客户端（Codex / Claude Code）的本地配置以增量、可回滚的方式接入 YeAPI。
//!
//! 这里只处理“本地配置文件维护”，不是 HTTP 微服务（见 `services/`）。

pub mod claude_code;
pub mod claude_desktop;
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
    Claude,
}

impl ClientKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim() {
            "codex" => Ok(Self::Codex),
            // 兼容早期草稿里用过的 claude_code 拼写。
            "claude" | "claude_code" => Ok(Self::Claude),
            other => Err(format!("暂不支持的客户端: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }

    pub fn default_key_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }
}

/// 一个客户端的用户级配置目录与关键文件。
pub struct ClientPaths {
    pub home: PathBuf,
    pub config: PathBuf,
    /// Codex 为 `auth.json`；Claude Code 的凭据由自身管理，接入流程不碰。
    pub auth: Option<PathBuf>,
}

pub fn resolve_paths(kind: ClientKind) -> Result<ClientPaths, String> {
    match kind {
        ClientKind::Codex => paths::resolve_codex_paths(),
        ClientKind::Claude => paths::resolve_claude_paths(),
    }
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
    pub config_home: String,
    pub config_path: String,
    pub current_model: Option<String>,
    pub applied_api_key_id: Option<String>,
    pub applied_api_key_hint: Option<String>,
    pub applied_api_key_valid: bool,
    pub applied_at: Option<String>,
    pub applied_base_url: Option<String>,
    pub gateway_base_url: String,
    pub base_url_matches: bool,
    pub backup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex: Option<CodexStatusDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeStatusDetail>,
}

#[derive(Debug, Serialize)]
pub struct CodexStatusDetail {
    pub auth_path: String,
    pub auth_logged_in: bool,
    pub requires_openai_auth: bool,
    pub current_model_provider: Option<String>,
    pub model_catalog_json: Option<String>,
    pub catalog_matches: bool,
    pub yeapi_provider_present: bool,
    pub provider_token_hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClaudeStatusDetail {
    pub env_present: bool,
    pub current_base_url: Option<String>,
    pub env_models: ClaudeEnvModels,
    pub top_level_model: Option<String>,
    pub anthropic_api_key_present: bool,
    pub desktop: ClaudeDesktopStatus,
}

#[derive(Debug, Default, Serialize)]
pub struct ClaudeEnvModels {
    pub opus: Option<String>,
    pub sonnet: Option<String>,
    pub haiku: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ClaudeDesktopStatus {
    pub detected: bool,
    pub config_dir: Option<String>,
    pub meta_path: Option<String>,
    pub third_party_mode: bool,
    pub applied_profile_name: Option<String>,
    pub applied_base_url: Option<String>,
    pub applied_models: Vec<String>,
    pub yeapi_profile_present: bool,
    pub applied: bool,
    pub drift: bool,
    pub base_url_matches: bool,
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
    #[serde(default)]
    pub claude: Option<ClaudeApplyOptions>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeApplyOptions {
    #[serde(default)]
    pub opus_model: Option<String>,
    #[serde(default)]
    pub sonnet_model: Option<String>,
    #[serde(default)]
    pub haiku_model: Option<String>,
    #[serde(default = "default_true")]
    pub apply_cli: bool,
    #[serde(default = "default_true")]
    pub apply_desktop: bool,
}

impl Default for ClaudeApplyOptions {
    fn default() -> Self {
        Self {
            opus_model: None,
            sonnet_model: None,
            haiku_model: None,
            apply_cli: true,
            apply_desktop: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Claude Code 的主模型与三个别名解析结果；空别名回退主模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeModelAliases {
    pub main: String,
    pub opus: String,
    pub sonnet: String,
    pub haiku: String,
}

impl ClaudeModelAliases {
    pub fn resolve(main: &str, options: Option<&ClaudeApplyOptions>) -> Self {
        let main = main.trim().to_string();
        let pick = |value: Option<&String>| {
            value
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .unwrap_or(main.as_str())
                .to_string()
        };
        Self {
            main: main.clone(),
            opus: pick(options.and_then(|o| o.opus_model.as_ref())),
            sonnet: pick(options.and_then(|o| o.sonnet_model.as_ref())),
            haiku: pick(options.and_then(|o| o.haiku_model.as_ref())),
        }
    }

    /// 去重后的全部模型（用于可用性校验）。
    pub fn distinct_models(&self) -> Vec<String> {
        let mut models = vec![
            self.main.clone(),
            self.opus.clone(),
            self.sonnet.clone(),
            self.haiku.clone(),
        ];
        models.sort();
        models.dedup();
        models
    }
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
    /// 模型在当前启用渠道里不可用时的提示（预览仍会生成）。
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<DesktopPreview>,
}

#[derive(Debug, Serialize)]
pub struct DesktopPreview {
    pub config_path: String,
    pub snippet: String,
    pub warning: Option<String>,
}

/// 预览配置时用于替代明文 Key 的占位符。
pub const MASKED_KEY: &str = "<已隐藏，写入时使用所选密钥>";

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {e}"))
}

/// `http://host:port`（Claude Code 的 `ANTHROPIC_BASE_URL` 与桌面端网关地址）。
pub(crate) async fn gateway_origin(app: &tauri::AppHandle, state: &crate::AppState) -> String {
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
    format!("http://{host}:{port}")
}

/// Codex 的 base_url 必须止于 `/v1`（客户端自己拼 `/responses`）。
pub(crate) async fn gateway_base_url(app: &tauri::AppHandle, state: &crate::AppState) -> String {
    format!("{}/v1", gateway_origin(app, state).await)
}

/// API Key 的掩码提示：只保留末 4 位。
pub(crate) fn key_hint(token: &str) -> String {
    let count = token.chars().count();
    if count <= 4 {
        return token.to_string();
    }
    let tail: String = token.chars().skip(count - 4).collect();
    format!("****{tail}")
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

    #[test]
    fn claude_apply_options_default_to_both_targets() {
        let options: ClaudeApplyOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(options.apply_cli);
        assert!(options.apply_desktop);
    }

    #[test]
    fn claude_aliases_fall_back_to_main() {
        let aliases = ClaudeModelAliases::resolve(
            "deepseek-v4-pro",
            Some(&ClaudeApplyOptions {
                opus_model: Some(" ".to_string()),
                sonnet_model: Some("deepseek-v4-flash".to_string()),
                haiku_model: None,
                apply_cli: true,
                apply_desktop: true,
            }),
        );
        assert_eq!(aliases.opus, "deepseek-v4-pro");
        assert_eq!(aliases.sonnet, "deepseek-v4-flash");
        assert_eq!(aliases.haiku, "deepseek-v4-pro");
    }

    #[test]
    fn key_hint_keeps_tail() {
        assert_eq!(key_hint("sk-yeapi-testkey"), "****tkey");
        assert_eq!(key_hint("abc"), "abc");
    }
}
