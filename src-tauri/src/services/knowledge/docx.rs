//! DOCX 解析：解包 `word/document.xml` → Markdown（标题 / 段落 / 表格）。

use std::io::{Cursor, Read};
use roxmltree::Node;

use super::table::rows_to_markdown;

/// 提取 .docx 文本为 Markdown。
pub fn extract_docx(content: &[u8]) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(content))
        .map_err(|e| format!("打开 docx 失败: {e}"))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| format!("docx 缺少 word/document.xml: {e}"))?
        .read_to_string(&mut xml)
        .map_err(|e| format!("读取 document.xml 失败: {e}"))?;

    let md = document_xml_to_markdown(&xml);
    if md.trim().is_empty() {
        return Err("无法从该 docx 提取文本".to_string());
    }
    Ok(md)
}

/// `word/document.xml` → Markdown（纯函数，供单测）。
fn document_xml_to_markdown(xml: &str) -> String {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let Some(body) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return String::new();
    };

    let mut blocks: Vec<String> = Vec::new();
    for node in body.children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "p" => {
                if let Some(p) = render_paragraph(&node) {
                    blocks.push(p);
                }
            }
            "tbl" => {
                if let Some(t) = render_table(&node) {
                    blocks.push(t);
                }
            }
            _ => {}
        }
    }
    blocks.join("\n\n")
}

fn render_paragraph(p: &Node) -> Option<String> {
    let text = collect_text(p);
    if text.is_empty() {
        return None;
    }
    if let Some(level) = paragraph_style(p).and_then(|s| heading_level(&s)) {
        return Some(format!("{} {}", "#".repeat(level), text));
    }
    Some(text)
}

fn render_table(tbl: &Node) -> Option<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in tbl
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "tr")
    {
        let cells: Vec<String> = tr
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "tc")
            .map(|tc| collect_text(&tc))
            .collect();
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return None;
    }
    match rows_to_markdown(&rows) {
        Some(md) => Some(md),
        None => Some(flatten_cells(&rows)),
    }
}

fn flatten_cells(rows: &[Vec<String>]) -> String {
    rows.iter()
        .flat_map(|r| r.iter())
        .filter(|c| !c.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ")
}

/// 段落/单元格内文本：按序拼接 `<w:t>`，`<w:tab>`→`\t`，`<w:br>`/`<w:cr>`→换行；
/// 忽略 `<w:instrText>`/`<w:delText>` 等域代码与删除文本（各自是独立标签，不匹配 `t`）。
fn collect_text(node: &Node) -> String {
    let mut s = String::new();
    for n in node.descendants() {
        if !n.is_element() {
            continue;
        }
        match n.tag_name().name() {
            "t" => {
                if let Some(t) = n.text() {
                    s.push_str(t);
                }
            }
            "tab" => s.push('\t'),
            "br" | "cr" => s.push('\n'),
            _ => {}
        }
    }
    s.trim().to_string()
}

/// 段落样式名（`<w:pStyle w:val="…">`）。
fn paragraph_style(p: &Node) -> Option<String> {
    p.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "pStyle")
        .and_then(|n| n.attribute("val"))
        .map(|s| s.to_string())
}

/// 样式名 → 标题层级（`Heading1..6` / `标题1..6`）。
fn heading_level(style: &str) -> Option<usize> {
    let s = style.trim();
    for level in 1..=6usize {
        if s == format!("Heading{level}") || s == format!("标题{level}") {
            return Some(level);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY_START: &str = r#"<?xml version="1.0"?><w:document xmlns:w="urn:x"><w:body>"#;
    const BODY_END: &str = "</w:body></w:document>";

    #[test]
    fn test_heading_and_paragraph() {
        let xml = format!(
            "{BODY_START}<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>个人信息</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Spring</w:t></w:r><w:r><w:t> Boot</w:t></w:r></w:p>{BODY_END}"
        );
        let md = document_xml_to_markdown(&xml);
        assert!(md.contains("# 个人信息"));
        assert!(md.contains("Spring Boot"));
    }

    #[test]
    fn test_chinese_heading_style() {
        let xml = format!(
            "{BODY_START}<w:p><w:pPr><w:pStyle w:val=\"标题2\"/></w:pPr><w:r><w:t>教育经历</w:t></w:r></w:p>{BODY_END}"
        );
        let md = document_xml_to_markdown(&xml);
        assert!(md.contains("## 教育经历"));
    }

    #[test]
    fn test_table() {
        let xml = format!(
            "{BODY_START}<w:tbl><w:tr><w:tc><w:p><w:r><w:t>技术</w:t></w:r></w:p></w:tc>\
            <w:tc><w:p><w:r><w:t>Spring</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>框架</w:t></w:r></w:p></w:tc>\
            <w:tc><w:p><w:r><w:t>Boot</w:t></w:r></w:p></w:tc></w:tr></w:tbl>{BODY_END}"
        );
        let md = document_xml_to_markdown(&xml);
        assert!(md.contains("| 技术 | Spring |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| 框架 | Boot |"));
    }

    #[test]
    fn test_ignores_deleted_text() {
        let xml = format!(
            "{BODY_START}<w:p><w:r><w:t>保留</w:t></w:r><w:r><w:delText>删除</w:delText></w:r></w:p>{BODY_END}"
        );
        let md = document_xml_to_markdown(&xml);
        assert!(md.contains("保留"));
        assert!(!md.contains("删除"));
    }
}
