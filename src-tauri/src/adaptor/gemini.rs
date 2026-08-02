use async_trait::async_trait;
use super::*;

pub struct GeminiAdaptor;

#[async_trait]
impl Adaptor for GeminiAdaptor {
    fn channel_type(&self) -> &'static str { "gemini" }
    fn default_models(&self) -> Vec<&'static str> {
        vec!["gemini-2.5-flash", "gemini-2.5-pro"]
    }
    fn default_base_url(&self) -> &str { "https://generativelanguage.googleapis.com" }

    async fn test(&self, _config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        Ok(TestResult {
            success: false,
            message: "Gemini adaptor not implemented yet".into(),
            latency_ms: 0,
        })
    }

    async fn forward(
        &self,
        _request: &ProxyRequest,
        _config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error> {
        Err(anyhow::anyhow!("Gemini adaptor not implemented yet"))
    }

    async fn forward_stream(
        &self,
        _request: &ProxyRequest,
        _config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        Err(anyhow::anyhow!("Gemini adaptor not implemented yet"))
    }
}
