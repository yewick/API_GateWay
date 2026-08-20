//! MinerU 后端（接口占位）。
//!
//! 面向复杂 PDF（扫描页 / 复杂表格 / 多栏 / 图片文字 / 公式 / 非标准阅读顺序）的
//! 自托管解析服务。本次仅实现 [`MinerUExtractor`] 的接口桩，未来接入时把 `extract`
//! 换成「reqwest 调用 MinerU-API + 服务地址/超时/鉴权」，分发层（`pdf.rs`）零改动。

use super::pdf::PdfExtractor;

/// MinerU 后端（待接入）
pub struct MinerUExtractor;

impl PdfExtractor for MinerUExtractor {
    fn extract(&self, _content: &[u8]) -> Result<String, String> {
        Err("MinerU 后端尚未实现（自托管 HTTP 服务，待接入）".to_string())
    }
}
