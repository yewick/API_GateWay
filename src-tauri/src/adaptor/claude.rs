use async_trait::async_trait;
use super::*;

pub struct ClaudeAdaptor;

#[async_trait]
impl Adaptor for ClaudeAdaptor {
    fn channel_type(&self) -> &'static str { "claude" }
    fn default_models(&self) -> Vec<&'static str> {
        vec!["claude-sonnet-4-20250514", "claude-3-5-haiku-20241022"]
    }
    fn default_base_url(&self) -> &str { "https://api.anthropic.com" }

    async fn test(&self, _config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        Ok(TestResult {
            success: false,
            message: "Claude adaptor not implemented yet".into(),
            latency_ms: 0,
        })
    }

    async fn forward(
        &self,
        _request: &ProxyRequest,
        _config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error> {
        Err(anyhow::anyhow!("Claude adaptor not implemented yet"))
    }

    async fn forward_stream(
        &self,
        _request: &ProxyRequest,
        _config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        Err(anyhow::anyhow!("Claude adaptor not implemented yet"))
    }
}
