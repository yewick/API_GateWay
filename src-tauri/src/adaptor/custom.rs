use async_trait::async_trait;
use super::*;

pub struct CustomAdaptor;

#[async_trait]
impl Adaptor for CustomAdaptor {
    fn channel_type(&self) -> &'static str { "custom" }
    fn default_models(&self) -> Vec<&'static str> { vec![] }
    fn default_base_url(&self) -> &str { "" }

    async fn test(&self, _config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        Ok(TestResult {
            success: false,
            message: "Custom adaptor — configure base_url and test manually".into(),
            latency_ms: 0,
        })
    }

    async fn forward(
        &self,
        _request: &ProxyRequest,
        _config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error> {
        Err(anyhow::anyhow!("Custom adaptor not configured"))
    }

    async fn forward_stream(
        &self,
        _request: &ProxyRequest,
        _config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        Err(anyhow::anyhow!("Custom adaptor not configured"))
    }
}
