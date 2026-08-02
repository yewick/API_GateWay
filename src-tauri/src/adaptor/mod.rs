pub mod openai;
pub mod claude;
pub mod gemini;
pub mod deepseek;
pub mod custom;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 渠道配置——从数据库 Channel 转换而来
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub model_mapping: serde_json::Value,
    pub extra: serde_json::Value,
}

/// 代理请求——统一的上游请求抽象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    pub model: String,
    pub body: serde_json::Value,
    pub stream: bool,
}

/// 渠道连通性测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: u64,
}

/// Token 用量——统一各家的计费格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[async_trait]
pub trait Adaptor: Send + Sync {
    /// 渠道类型标识
    fn channel_type(&self) -> &'static str;
    /// 默认支持的模型列表
    fn default_models(&self) -> Vec<&'static str>;
    /// 默认 API 地址
    fn default_base_url(&self) -> &str;

    /// 测试渠道连通性
    async fn test(&self, config: &ChannelConfig) -> Result<TestResult, anyhow::Error>;

    /// 非流式转发：返回 (状态码, 响应体, Token用量)
    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error>;

    /// 流式转发：直接返回 reqwest::Response，由调用方逐字节转发 SSE
    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error>;
}

pub fn get_adaptor(channel_type: &str) -> Box<dyn Adaptor> {
    match channel_type {
        "openai" => Box::new(openai::OpenAIAdaptor),
        "deepseek" => Box::new(deepseek::DeepSeekAdaptor),
        "claude" => Box::new(claude::ClaudeAdaptor),
        "gemini" => Box::new(gemini::GeminiAdaptor),
        "custom" => Box::new(custom::CustomAdaptor),
        _ => Box::new(custom::CustomAdaptor),  // 未知类型兜底走自定义
    }
}

pub fn channel_types() -> Vec<ChannelTypeInfo> {
    vec![
        ChannelTypeInfo { value: "openai", label: "OpenAI", category: "international",
            default_base_url: "https://api.openai.com/v1",
            models: vec!["gpt-5.4", "gpt-5.5", "gpt-4o", "gpt-4o-mini"] },
        ChannelTypeInfo { value: "deepseek", label: "DeepSeek", category: "international",
            default_base_url: "https://api.deepseek.com/v1",
            models: vec!["deepseek-v4-flash", "deepseek-v4-pro"] },
        ChannelTypeInfo { value: "claude", label: "Anthropic Claude", category: "international",
            default_base_url: "https://api.anthropic.com",
            models: vec!["claude-sonnet-4-20250514", "claude-3-5-haiku-20241022"] },
        ChannelTypeInfo { value: "gemini", label: "Google Gemini", category: "international",
            default_base_url: "https://generativelanguage.googleapis.com",
            models: vec!["gemini-2.5-flash", "gemini-2.5-pro"] },
        ChannelTypeInfo { value: "qwen", label: "通义千问", category: "domestic",
            default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            models: vec!["qwen-max", "qwen-plus"] },
        // ... 智谱、Moonshot、豆包、Ollama、自定义
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTypeInfo {
    pub value: &'static str,
    pub label: &'static str,
    pub category: &'static str,       // international | domestic | local | custom
    pub default_base_url: &'static str,
    pub models: Vec<&'static str>,
}