//! HTML 解析：html2text → 纯文本。

/// 提取 .html/.htm 文本。
pub fn extract_html(content: &[u8]) -> Result<String, String> {
    let text = html2text::from_read(&content[..], 10_000)
        .map_err(|e| format!("HTML 解析失败: {e}"))?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("无法从该 HTML 提取文本".to_string());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_html() {
        let html = "<html><body><h1>标题</h1><p>正文内容</p></body></html>";
        let text = extract_html(html.as_bytes()).unwrap();
        assert!(text.contains("标题"));
        assert!(text.contains("正文内容"));
    }

    #[test]
    fn test_empty_html_errors() {
        assert!(extract_html(b"<html><body></body></html>").is_err());
    }
}
