//! PPTX 解析：解包 `ppt/slides/slide*.xml` → 每页段落文本。

use std::io::{Cursor, Read};

/// 提取 .pptx 文本（纯文本，页间空行分隔）。
pub fn extract_pptx(content: &[u8]) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(content))
        .map_err(|e| format!("打开 pptx 失败: {e}"))?;

    let mut slides: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let name = archive
            .by_index(i)
            .map_err(|e| format!("读取 pptx 条目失败: {e}"))?
            .name()
            .to_string();
        if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            slides.push(name);
        }
    }
    slides.sort_by_key(|n| slide_number(n));

    let mut out: Vec<String> = Vec::new();
    for name in slides {
        let mut xml = String::new();
        archive
            .by_name(&name)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?
            .read_to_string(&mut xml)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?;
        let text = slide_xml_to_text(&xml);
        if !text.trim().is_empty() {
            out.push(text);
        }
    }

    if out.is_empty() {
        return Err("无法从该 pptx 提取文本".to_string());
    }
    Ok(out.join("\n\n"))
}

/// 从 `slideN.xml` 文件名提取数字序号（`slide10.xml` → 10）。
fn slide_number(name: &str) -> u32 {
    name.rsplit("slide")
        .next()
        .and_then(|rest| rest.split('.').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// `slideN.xml` → 段落文本（纯函数，供单测）。
fn slide_xml_to_text(xml: &str) -> String {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let mut paras: Vec<String> = Vec::new();
    for p in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "p")
    {
        let text: String = p
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "t")
            .filter_map(|n| n.text())
            .collect();
        let text = text.trim().to_string();
        if !text.is_empty() {
            paras.push(text);
        }
    }
    paras.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_number() {
        assert_eq!(slide_number("ppt/slides/slide1.xml"), 1);
        assert_eq!(slide_number("ppt/slides/slide10.xml"), 10);
    }

    #[test]
    fn test_slide_xml_to_text() {
        let xml = r#"<?xml version="1.0"?><p:sld xmlns:p="urn:p" xmlns:a="urn:a">
            <p:sp><p:txBody><a:p><a:r><a:t>Hello</a:t></a:r><a:r><a:t> World</a:t></a:r></a:p>
            <a:p><a:r><a:t>第二行</a:t></a:r></a:p></p:txBody></p:sp></p:sld>"#;
        let text = slide_xml_to_text(xml);
        assert!(text.contains("Hello World"));
        assert!(text.contains("第二行"));
    }
}
