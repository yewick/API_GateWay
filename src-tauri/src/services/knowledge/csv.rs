//! CSV 解析：csv crate → Markdown 表格。

use super::table::rows_to_markdown;

/// 提取 .csv 文本为 Markdown 表格。
pub fn extract_csv(content: &[u8]) -> Result<String, String> {
    // 去掉 UTF-8 BOM（Windows/Excel 导出常见）
    let bytes = content.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(content);

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(bytes);

    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| format!("CSV 解析失败: {e}"))?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }

    if rows.is_empty() {
        return Err("无法从该 CSV 提取内容".to_string());
    }
    Ok(match rows_to_markdown(&rows) {
        Some(md) => md,
        None => rows
            .into_iter()
            .flat_map(|r| r)
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join(" | "),
    })
}
