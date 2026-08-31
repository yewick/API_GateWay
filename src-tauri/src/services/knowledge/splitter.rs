//! 文本分块：把解析后的纯文本切成可检索的最小单元（chunk）。
//!
//! 三种策略：通用文本（`split_text`）、Markdown 按标题（`split_markdown`）、
//! 代码按符号边界（`split_code_by_symbols`）。统一入口 [`split_document`]。
//!
//! Token 估算采用简单启发式 ~4 字符/token，避免引入重量级 tokenizer。

use serde::Serialize;

use super::code_parser::{extract_symbols, Symbol};
use super::parser::ParsedDocument;

/// 分块配置（`chunk_size`/`chunk_overlap` 单位为 token）
#[derive(Debug, Clone)]
pub struct SplitConfig {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            chunk_size: 512,
            chunk_overlap: 64,
        }
    }
}

/// 分块元数据（标题 / 符号 / 行号等）
#[derive(Debug, Clone, Default, Serialize)]
pub struct ChunkMetadata {
    pub heading: Option<String>,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub signature: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
}

/// 一个分块
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub token_count: usize,
    pub metadata: ChunkMetadata,
}

/// 启发式 token 估算：约 4 字符 = 1 token
pub fn token_count(text: &str) -> usize {
    (text.chars().count() + 3) / 4
}

/// 分块分发器：按文档类型选择策略。
pub fn split_document(parsed: &ParsedDocument, config: &SplitConfig) -> Vec<Chunk> {
    let base = ChunkMetadata::default();
    match parsed.file_type.as_str() {
        "markdown" => split_markdown(&parsed.text, config, &base),
        "code" => {
            // tree-sitter 符号提取：按语言解析真实符号后按符号边界分块；
            // 未支持的语言返回空 → split_code_by_symbols 内部回退 split_text。
            let symbols = parsed
                .language
                .as_deref()
                .map(|lang| extract_symbols(lang, &parsed.text))
                .unwrap_or_default();
            split_code_by_symbols(&parsed.text, &symbols, config, &base)
        }
        _ => split_text(&parsed.text, config, &base),
    }
}

/// 父块分组结果：连续子块区间（左闭右开）+ 拼接后的父块正文。
#[derive(Debug, Clone)]
pub struct ParentGroup {
    /// 子块区间（按 `chunks` 下标）
    pub child_range: std::ops::Range<usize>,
    pub content: String,
    pub token_count: usize,
}

/// 把有序子块按累积 token 贪心聚合成父块（Parent/Child 检索的上下文补全）。
///
/// 目标：每个父块约 `parent_target_tokens`（建议 `chunk_size*4`），单父块最多 `max_children` 个子块；
/// 父块正文 = 其子块正文以 `\n\n` 拼接。空列表返回空；单子块超目标时退化为「一子一父」。
pub fn build_parents(
    chunks: &[Chunk],
    parent_target_tokens: usize,
    max_children: usize,
) -> Vec<ParentGroup> {
    if chunks.is_empty() {
        return Vec::new();
    }
    let target = parent_target_tokens.max(1);
    let max_children = max_children.max(1);

    let mut groups: Vec<ParentGroup> = Vec::new();
    let mut start = 0usize;
    let mut acc_tokens = 0usize;
    let mut count = 0usize;

    for (i, c) in chunks.iter().enumerate() {
        let ct = c.token_count.max(1);
        // 当前组非空且再塞一个会超 token 目标或子块数上限 → 封组，另起新组
        if count > 0 && (acc_tokens + ct > target || count >= max_children) {
            groups.push(finalize_parent(chunks, start, i));
            start = i;
            acc_tokens = 0;
            count = 0;
        }
        acc_tokens += ct;
        count += 1;
    }
    if count > 0 {
        groups.push(finalize_parent(chunks, start, chunks.len()));
    }
    groups
}

fn finalize_parent(chunks: &[Chunk], start: usize, end: usize) -> ParentGroup {
    let content = chunks[start..end]
        .iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    ParentGroup {
        child_range: start..end,
        token_count: token_count(&content),
        content,
    }
}

/// 通用文本分块：按行累积 token，超限 flush，保留重叠内容。
///
/// 额外识别三类「应保持原子」的结构，避免被普通按行累积从中间切碎：
/// - Markdown 表格（连续 `|` 行）：整表不切；超大时按数据行切、每段重复表头。
/// - HTML 表格（`<table>`…`</table>`）：整表不切；超大时按 `</tr>` 切、重复表头。
/// - 连续键值对（`key: value` 短行，≥2 行）：独立成块，不与正文混切。
pub fn split_text(content: &str, config: &SplitConfig, metadata: &ChunkMetadata) -> Vec<Chunk> {
    if content.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content.split('\n').collect();
    let target_chars = config.chunk_size.saturating_mul(4);
    let overlap_chars = config.chunk_overlap.saturating_mul(4);

    let mut chunks = Vec::new();
    let mut start = 0usize; // 当前块起始的绝对行号
    let mut current: Vec<&str> = Vec::new();
    let mut current_chars = 0usize;

    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];

        // 特殊块：HTML 表格（跨行，含单行内闭合的情况）
        if is_html_table_open(line) {
            if !current.is_empty() {
                chunks.push(build_chunk(&current, metadata, start, i - 1));
                current = Vec::new();
                current_chars = 0;
            }
            let block_start = i;
            let mut j = i;
            while j < lines.len() {
                let closed = is_html_table_close(lines[j]);
                j += 1;
                if closed {
                    break;
                }
            }
            push_html_table(&lines[block_start..j], config, metadata, block_start, &mut chunks);
            i = j;
            start = j;
            continue;
        }

        // 特殊块：Markdown 表格（≥2 行连续 `|` 行）
        if is_md_table_line(line) {
            let mut j = i;
            while j < lines.len() && is_md_table_line(lines[j]) {
                j += 1;
            }
            if j - i >= 2 {
                if !current.is_empty() {
                    chunks.push(build_chunk(&current, metadata, start, i - 1));
                    current = Vec::new();
                    current_chars = 0;
                }
                push_md_table(&lines[i..j], config, metadata, i, &mut chunks);
                i = j;
                start = j;
                continue;
            }
        }

        // 特殊块：连续键值对（≥2 行）
        if is_kv_line(line) {
            let mut j = i;
            while j < lines.len() && is_kv_line(lines[j]) {
                j += 1;
            }
            if j - i >= 2 {
                if !current.is_empty() {
                    chunks.push(build_chunk(&current, metadata, start, i - 1));
                    current = Vec::new();
                    current_chars = 0;
                }
                push_kv_block(&lines[i..j], config, metadata, i, &mut chunks);
                i = j;
                start = j;
                continue;
            }
        }

        let line_chars = line.chars().count();

        // 单行超长：先 flush 已累积块，再按字符硬切，避免与 overlap 回退死循环
        if line_chars > target_chars {
            if !current.is_empty() {
                chunks.push(build_chunk(&current, metadata, start, i - 1));
                current = Vec::new();
                current_chars = 0;
            }
            let mut off = 0usize;
            while off < line_chars {
                let seg: String = line.chars().skip(off).take(target_chars).collect();
                chunks.push(Chunk {
                    token_count: token_count(&seg),
                    content: seg,
                    metadata: ChunkMetadata {
                        line_start: i,
                        line_end: i,
                        ..metadata.clone()
                    },
                });
                off += target_chars;
            }
            i += 1;
            start = i;
            continue;
        }

        // 当前块装不下该行 → flush，再按 overlap 回退
        if !current.is_empty() && current_chars + line_chars > target_chars {
            chunks.push(build_chunk(&current, metadata, start, i - 1));

            // 保留末尾约 overlap_chars 的重叠内容
            let mut tail = current.len();
            let mut tail_chars = 0usize;
            for idx in (0..current.len()).rev() {
                let c = current[idx].chars().count() + 1;
                if tail_chars + c > overlap_chars && tail != current.len() {
                    break;
                }
                tail_chars += c;
                tail = idx;
            }
            start += tail;
            current = current[tail..].to_vec();
            current_chars = tail_chars;
            // 行本身过大，overlap 后仍放不下 → 放弃 overlap 另起一块（否则死循环）
            if current_chars + line_chars > target_chars {
                current = Vec::new();
                current_chars = 0;
                start = i;
            }
            continue; // 重新处理同一行
        }

        current.push(line);
        current_chars += line_chars + 1; // +1 为换行符
        i += 1;
    }

    if !current.is_empty() {
        chunks.push(build_chunk(&current, metadata, start, lines.len() - 1));
    }

    chunks
}

/// Markdown 分块：按标题切段，每段一个 chunk；标题路径只写入元数据（`heading`），
/// 不拼进正文，避免顶层标题（如「宁波海曙生命医疗科技有限公司」）在每块正文重复、稀释向量。
/// 纯标题段（无正文）跳过；超大段落递归 split_text（内部保留表格/键值对原子性）。
pub fn split_markdown(
    content: &str,
    config: &SplitConfig,
    metadata: &ChunkMetadata,
) -> Vec<Chunk> {
    if content.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content.split('\n').collect();
    let max_section_chars = config.chunk_size.saturating_mul(4).saturating_mul(2);

    // 收集 ATX 标题行（#{1,6} 后接空格/行尾），记录层级
    let mut headings: Vec<(usize, usize, String)> = Vec::new(); // (行号, 层级, 文本)
    for (idx, line) in lines.iter().enumerate() {
        if let Some((level, text)) = parse_heading(line) {
            headings.push((idx, level, text));
        }
    }

    if headings.is_empty() {
        return split_text(content, config, metadata);
    }

    // 标题栈：栈内即当前标题的完整祖先路径（对每个标题维护，含无正文的父标题）
    let mut stack: Vec<(usize, String)> = Vec::new(); // (层级, 文本)

    let mut chunks = Vec::new();

    // preamble（首标题之前的内容），无标题
    if headings[0].0 > 0 {
        let text = lines[..headings[0].0].join("\n");
        if !text.trim().is_empty() {
            chunks.push(Chunk {
                token_count: token_count(&text),
                content: text.clone(),
                metadata: ChunkMetadata {
                    line_start: 0,
                    line_end: headings[0].0 - 1,
                    ..metadata.clone()
                },
            });
        }
    }

    for (h, (start, level, heading_text)) in headings.iter().enumerate() {
        let (start, level) = (*start, *level);
        let body_end = headings
            .get(h + 1)
            .map(|(next, _, _)| *next)
            .unwrap_or(lines.len());

        // 维护标题栈：弹出层级 >= 当前的项，压入当前标题（无论本段是否有正文）
        while matches!(stack.last(), Some((lv, _)) if *lv >= level) {
            stack.pop();
        }
        stack.push((level, heading_text.clone()));

        // 正文 = 标题行之后、下一标题行之前；纯标题段跳过
        let body = lines[start + 1..body_end].join("\n");
        if body.trim().is_empty() {
            continue;
        }

        // 完整标题路径只作元数据，不拼进正文
        let header: String = stack
            .iter()
            .map(|(lv, t)| format!("{} {}", "#".repeat(*lv), t))
            .collect::<Vec<_>>()
            .join("\n");

        if body.chars().count() <= max_section_chars {
            chunks.push(Chunk {
                token_count: token_count(&body),
                content: body,
                metadata: ChunkMetadata {
                    heading: Some(header.clone()),
                    line_start: start,
                    line_end: body_end.saturating_sub(1),
                    ..metadata.clone()
                },
            });
        } else {
            // 超大段落：递归 split_text（内部保留表格/键值对原子性）
            for c in split_text(&body, config, metadata) {
                chunks.push(Chunk {
                    token_count: c.token_count,
                    content: c.content,
                    metadata: ChunkMetadata {
                        heading: Some(header.clone()),
                        line_start: start + 1 + c.metadata.line_start,
                        line_end: start + 1 + c.metadata.line_end,
                        ..metadata.clone()
                    },
                });
            }
        }
    }

    chunks
}

/// 判断是否为 Markdown 表格行（trim 后以 `|` 开头）。
fn is_md_table_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// 判断是否为 Markdown 表头分隔行（`| --- | --- |`）。
fn is_md_separator(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.contains("---")
}

/// 判断是否为 HTML 表格开/闭标签（含单行内 `<table>…</table>` 的情形）。
fn is_html_table_open(line: &str) -> bool {
    line.to_ascii_lowercase().contains("<table")
}

fn is_html_table_close(line: &str) -> bool {
    line.to_ascii_lowercase().contains("</table")
}

/// 判断是否为「键: 值」短行（用于把连续规格词条独立成块）。
/// 保守匹配：键无空白/斜杠且短、值较短且不以句末标点结尾，避免把「结论：该方法有效。」这类正文句误判。
fn is_kv_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.contains("://") {
        return false;
    }
    let Some(colon) = t.find([':', '：']) else {
        return false;
    };
    // 冒号可能是全角（3 字节 UTF-8），按字符长度跳过，避免切到 char 边界内
    let colon_len = t[colon..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    let key = t[..colon].trim();
    let val = t[colon + colon_len..].trim();
    if key.is_empty() || val.is_empty() {
        return false;
    }
    if key.chars().count() > 24 || val.chars().count() > 64 {
        return false;
    }
    if key.chars().any(char::is_whitespace) || key.contains('/') {
        return false;
    }
    if let Some(last) = val.chars().last() {
        if matches!(last, '。' | '；' | ';' | '.') {
            return false;
        }
    }
    true
}

/// Markdown 表格落块：整表原子；超大时按数据行切、每段重复表头 + 分隔行。
fn push_md_table(
    block: &[&str],
    config: &SplitConfig,
    metadata: &ChunkMetadata,
    line_start: usize,
    out: &mut Vec<Chunk>,
) {
    let header_rows = if block.len() >= 2 && is_md_separator(block[1]) { 2 } else { 1 };
    let header = block[..header_rows].join("\n");
    let data = &block[header_rows..];
    let target_chars = config.chunk_size.saturating_mul(4);
    let header_chars = header.chars().count();
    let first_data_line = line_start + header_rows;

    let mut buf: Vec<&str> = Vec::new();
    let mut buf_chars = 0usize;
    let mut row_start = first_data_line;
    for (j, row) in data.iter().enumerate() {
        let rc = row.chars().count();
        if !buf.is_empty() && buf_chars + rc + header_chars + 1 > target_chars {
            out.push(md_table_chunk(&header, &buf, metadata, row_start, row_start + buf.len() - 1));
            buf.clear();
            buf_chars = 0;
            row_start = first_data_line + j;
        }
        buf.push(*row);
        buf_chars += rc + 1;
    }
    if !buf.is_empty() {
        out.push(md_table_chunk(&header, &buf, metadata, row_start, row_start + buf.len() - 1));
    }
}

fn md_table_chunk(
    header: &str,
    rows: &[&str],
    metadata: &ChunkMetadata,
    line_start: usize,
    line_end: usize,
) -> Chunk {
    let content = format!("{header}\n{}", rows.join("\n"));
    Chunk {
        token_count: token_count(&content),
        content,
        metadata: ChunkMetadata { line_start, line_end, ..metadata.clone() },
    }
}

/// HTML 表格落块：整表原子；超大时按 `</tr>` 切数据行、重复表头（首个 `</tr>` 之前的内容）。
fn push_html_table(
    block: &[&str],
    config: &SplitConfig,
    metadata: &ChunkMetadata,
    line_start: usize,
    out: &mut Vec<Chunk>,
) {
    let text = block.join("\n");
    let line_end = line_start + block.len().saturating_sub(1);
    let target_chars = config.chunk_size.saturating_mul(4);
    if text.chars().count() <= target_chars {
        out.push(Chunk {
            token_count: token_count(&text),
            content: text,
            metadata: ChunkMetadata { line_start, line_end, ..metadata.clone() },
        });
        return;
    }

    // 超大 HTML 表（极少见）：表头 = 首个 `</tr>` 之前；数据行按 `</tr>` 切并重复表头。
    let Some(header_end) = text.find("</tr>").map(|p| p + "</tr>".len()) else {
        out.push(Chunk {
            token_count: token_count(&text),
            content: text,
            metadata: ChunkMetadata { line_start, line_end, ..metadata.clone() },
        });
        return;
    };
    let header = text[..header_end].to_string();
    let header_chars = header.chars().count();
    let mut body_rows: Vec<String> = Vec::new();
    let mut body_chars = 0usize;

    for seg in text[header_end..].split("</tr>") {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let row = format!("{seg}</tr>");
        let rc = row.chars().count();
        if !body_rows.is_empty() && body_chars + rc + header_chars + 1 > target_chars {
            out.push(html_table_chunk(&header, &body_rows, metadata, line_start, line_end));
            body_rows.clear();
            body_chars = 0;
        }
        body_chars += rc + 1;
        body_rows.push(row);
    }
    if !body_rows.is_empty() {
        out.push(html_table_chunk(&header, &body_rows, metadata, line_start, line_end));
    }
}

fn html_table_chunk(
    header: &str,
    rows: &[String],
    metadata: &ChunkMetadata,
    line_start: usize,
    line_end: usize,
) -> Chunk {
    let content = format!("{header}\n{}", rows.join("\n"));
    Chunk {
        token_count: token_count(&content),
        content,
        metadata: ChunkMetadata { line_start, line_end, ..metadata.clone() },
    }
}

/// 连续键值对落块：独立成块（不含 overlap），超长按行切。
fn push_kv_block(
    block: &[&str],
    config: &SplitConfig,
    metadata: &ChunkMetadata,
    line_start: usize,
    out: &mut Vec<Chunk>,
) {
    let target_chars = config.chunk_size.saturating_mul(4);
    let mut buf: Vec<&str> = Vec::new();
    let mut buf_chars = 0usize;
    let mut row_start = line_start;
    for (j, line) in block.iter().enumerate() {
        let rc = line.chars().count();
        if !buf.is_empty() && buf_chars + rc + 1 > target_chars {
            out.push(build_chunk(&buf, metadata, row_start, row_start + buf.len() - 1));
            buf.clear();
            buf_chars = 0;
            row_start = line_start + j;
        }
        buf.push(*line);
        buf_chars += rc + 1;
    }
    if !buf.is_empty() {
        out.push(build_chunk(&buf, metadata, row_start, row_start + buf.len() - 1));
    }
}

/// 解析 ATX 标题行：`#{1,6}` 后接空格/制表符或行尾（排除 `#hashtag`、`#!` 等误判）。
/// 返回 `(层级, 标题文本)`，非标题行返回 `None`。
fn parse_heading(line: &str) -> Option<(usize, String)> {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &t[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    Some((hashes, rest.trim().to_string()))
}

/// 代码按符号边界分块：每个符号一个完整 chunk，未覆盖行归为孤儿块，按行号排序。
///
/// `symbols` 为空时回退 [`split_text`]。
pub fn split_code_by_symbols(
    content: &str,
    symbols: &[Symbol],
    config: &SplitConfig,
    metadata: &ChunkMetadata,
) -> Vec<Chunk> {
    if symbols.is_empty() {
        return split_text(content, config, metadata);
    }

    let lines: Vec<&str> = content.split('\n').collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let last = lines.len() - 1;
    let max_symbol_chars = config.chunk_size.saturating_mul(4).saturating_mul(2);

    let mut syms: Vec<&Symbol> = symbols.iter().collect();
    syms.sort_by_key(|s| s.start_line);

    let mut chunks = Vec::new();
    let mut covered = vec![false; lines.len()];

    for s in &syms {
        let start = s.start_line.min(last);
        let end = s.end_line.min(last);
        for l in start..=end {
            covered[l] = true;
        }

        let text = lines[start..=end].join("\n");
        let symbol_meta = ChunkMetadata {
            symbol_name: Some(s.name.clone()),
            symbol_kind: Some(s.kind.as_str().to_string()),
            signature: s.signature.clone(),
            line_start: start,
            line_end: end,
            ..metadata.clone()
        };

        if text.chars().count() <= max_symbol_chars {
            chunks.push(Chunk {
                content: text.clone(),
                token_count: token_count(&text),
                metadata: symbol_meta,
            });
        } else {
            // 超大符号：内部再 split_text，行号加上偏移
            for c in split_text(&text, config, metadata) {
                chunks.push(Chunk {
                    content: c.content,
                    token_count: c.token_count,
                    metadata: ChunkMetadata {
                        symbol_name: symbol_meta.symbol_name.clone(),
                        symbol_kind: symbol_meta.symbol_kind.clone(),
                        signature: symbol_meta.signature.clone(),
                        line_start: start + c.metadata.line_start,
                        line_end: start + c.metadata.line_end,
                        ..metadata.clone()
                    },
                });
            }
        }
    }

    // 未被符号覆盖的行（import / 全局变量 / 注释）→ 孤儿块
    let mut orphan_chunks = Vec::new();
    let mut orphan_start: Option<usize> = None;
    for (i, &cov) in covered.iter().enumerate() {
        match (cov, orphan_start) {
            (false, None) => orphan_start = Some(i),
            (true, Some(s)) => {
                orphan_start = None;
                push_orphan(&lines[s..i], s, metadata, &mut orphan_chunks);
            }
            _ => {}
        }
    }
    if let Some(s) = orphan_start {
        push_orphan(&lines[s..], s, metadata, &mut orphan_chunks);
    }

    chunks.extend(orphan_chunks);
    chunks.sort_by_key(|c| c.metadata.line_start);
    chunks
}

/// 组装一个文本 chunk
fn build_chunk(lines: &[&str], metadata: &ChunkMetadata, line_start: usize, line_end: usize) -> Chunk {
    let content = lines.join("\n");
    Chunk {
        token_count: token_count(&content),
        content,
        metadata: ChunkMetadata {
            line_start,
            line_end,
            ..metadata.clone()
        },
    }
}

/// 把孤儿行区间转成一个 chunk（空内容跳过）
fn push_orphan(
    lines: &[&str],
    line_start: usize,
    metadata: &ChunkMetadata,
    out: &mut Vec<Chunk>,
) {
    let text = lines.join("\n");
    if text.trim().is_empty() {
        return;
    }
    let line_end = line_start + lines.len().saturating_sub(1);
    out.push(Chunk {
        content: text.clone(),
        token_count: token_count(&text),
        metadata: ChunkMetadata {
            line_start,
            line_end,
            ..metadata.clone()
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::knowledge::code_parser::SymbolKind;

    fn cfg() -> SplitConfig {
        SplitConfig {
            chunk_size: 8, // 8 token ≈ 32 字符
            chunk_overlap: 2, // 2 token ≈ 8 字符
        }
    }

    #[test]
    fn test_token_count() {
        assert_eq!(token_count(""), 0);
        assert_eq!(token_count("1234"), 1);
        assert_eq!(token_count("12345"), 2);
        assert_eq!(token_count("12345678"), 2);
    }

    #[test]
    fn test_split_text_basic() {
        let content = "line1\nline2\nline3\nline4\nline5\nline6";
        let chunks = split_text(content, &cfg(), &ChunkMetadata::default());
        assert!(!chunks.is_empty());
        // 所有内容拼接后应覆盖原文
        assert!(chunks.len() >= 2);
        // 每个 chunk 行号连续
        for c in &chunks {
            assert!(c.metadata.line_start <= c.metadata.line_end);
            assert!(c.token_count > 0);
        }
    }

    #[test]
    fn test_split_text_respects_overlap() {
        // 每行约 8 字符 → 每块约 4 行；overlap=8字符≈1行
        let content: String = (0..12)
            .map(|i| format!("row{:03}xx", i))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = split_text(&content, &cfg(), &ChunkMetadata::default());
        assert!(chunks.len() >= 2, "应产生多个块，实际 {}", chunks.len());
        // 相邻块之间应有重叠：前一块末尾行号 >= 后一块起始行号
        for w in chunks.windows(2) {
            assert!(w[0].metadata.line_end >= w[1].metadata.line_start);
        }
    }

    #[test]
    fn test_split_text_oversized_line_terminates() {
        // 回归：短行 + 超长行曾因 overlap 回退与超长行冲突而死循环/OOM（SIGKILL）
        let long = "x".repeat(100); // > 32 字符（target_chars）
        let content = format!("short\n{long}");
        let chunks = split_text(&content, &cfg(), &ChunkMetadata::default());
        // 能返回即说明已终止；长行被硬切成多块 + 短行一块
        assert!(chunks.len() >= 3, "实际 {}", chunks.len());
        assert!(chunks.iter().any(|c| c.content.contains('x')));
        assert!(chunks.iter().any(|c| c.content.contains("short")));
    }

    #[test]
    fn test_split_text_markdown_table_atomic() {
        // 小表格应整表一块，不因按行累积被拆开
        let cfg = SplitConfig { chunk_size: 20, chunk_overlap: 0 };
        let content = "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n| 5 | 6 |";
        let chunks = split_text(content, &cfg, &ChunkMetadata::default());
        assert_eq!(chunks.len(), 1, "实际 {:?}", chunks.len());
        let c = &chunks[0].content;
        assert!(c.contains("| A | B |"));
        assert!(c.contains("| 5 | 6 |"));
    }

    #[test]
    fn test_split_text_markdown_table_header_repeated() {
        // 超大表格按数据行切，但每块都重复表头 + 分隔行（修复 GSPR 表头丢失）
        let cfg = SplitConfig { chunk_size: 8, chunk_overlap: 0 }; // 32 字符
        let content = "| A | B |\n| --- | --- |\n| 111 | 222 |\n| 333 | 444 |\n| 555 | 666 |";
        let chunks = split_text(content, &cfg, &ChunkMetadata::default());
        assert!(chunks.len() >= 2, "实际 {}", chunks.len());
        for c in &chunks {
            assert!(
                c.content.starts_with("| A | B |\n| --- | --- |"),
                "每块都应带表头: {}",
                c.content
            );
        }
    }

    #[test]
    fn test_split_text_html_table_atomic() {
        let cfg = SplitConfig { chunk_size: 20, chunk_overlap: 0 };
        let content = "前文\n<table><tr><td>A</td><td>B</td></tr></table>\n后文";
        let chunks = split_text(content, &cfg, &ChunkMetadata::default());
        // HTML 表格（即使单行内闭合）应原子成块，不被从中间切
        assert!(
            chunks.iter().any(|c| c.content.contains("<table>") && c.content.contains("</table>")),
            "实际 {:?}",
            chunks.iter().map(|c| &c.content).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_split_text_kv_grouped() {
        // 连续键值对应独立成块，不与前后正文混切
        let cfg = SplitConfig { chunk_size: 20, chunk_overlap: 0 };
        let content = "说明文字\nTipSize: 20G\nLength: 30mm\nGauge: 0.4\n结尾段落";
        let chunks = split_text(content, &cfg, &ChunkMetadata::default());
        assert!(
            chunks.iter().any(|c| {
                c.content.contains("TipSize: 20G")
                    && c.content.contains("Length: 30mm")
                    && c.content.contains("Gauge: 0.4")
            }),
            "实际 {:?}",
            chunks.iter().map(|c| &c.content).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_split_text_kv_prose_not_grouped() {
        // 正文句「结论：该方法有效。」不应被误判为键值对
        let cfg = SplitConfig { chunk_size: 20, chunk_overlap: 0 };
        let content = "结论：该方法有效。\n进一步说明";
        let chunks = split_text(content, &cfg, &ChunkMetadata::default());
        // 两句应合为一块（按普通行累积），而非各自独立
        assert!(chunks.len() <= 2, "实际 {}", chunks.len());
    }

    #[test]
    fn test_split_markdown_by_heading() {
        let content = "# 标题一\n内容一\n内容二\n\n## 标题二\n更多内容";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert_eq!(chunks.len(), 2);
        // 标题路径只进元数据，不进正文
        assert_eq!(chunks[0].metadata.heading.as_deref(), Some("# 标题一"));
        assert_eq!(chunks[1].metadata.heading.as_deref(), Some("# 标题一\n## 标题二"));
        assert!(chunks[0].content.contains("内容一"));
        assert!(!chunks[0].content.contains("# 标题一"));
    }

    #[test]
    fn test_split_markdown_nested_skips_title_only() {
        // 父标题无正文（紧接子标题）→ 不再产出「只有标题没有描述」的 chunk
        let content = "# 父标题\n## 子标题\n正文内容";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert_eq!(chunks.len(), 1, "实际 {:?}", chunks.len());
        assert_eq!(chunks[0].metadata.heading.as_deref(), Some("# 父标题\n## 子标题"));
        // 正文不再携带标题前缀（去噪音）
        assert!(chunks[0].content.contains("正文内容"));
        assert!(!chunks[0].content.contains("父标题"));
    }

    #[test]
    fn test_split_markdown_heading_detection_strict() {
        // `#hashtag`（# 后无空格）不是标题 → 回退 split_text
        let content = "#hashtag\n正文";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.metadata.heading.is_none()));
    }

    #[test]
    fn test_split_markdown_preamble_kept() {
        // 首标题前的 preamble 独立成块（无标题元数据）
        let content = "开头说明\n\n# 标题\n正文";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].content.contains("开头说明"));
        assert!(chunks[0].metadata.heading.is_none());
        assert_eq!(chunks[1].metadata.heading.as_deref(), Some("# 标题"));
    }

    #[test]
    fn test_split_markdown_falls_back_without_heading() {
        let content = "没有标题\n的纯文本\n第三行";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.metadata.heading.is_none()));
    }

    #[test]
    fn test_split_code_by_symbols() {
        let content = "// import 注释\nuse x;\n\nfn foo() {\n  a\n  b\n}\n\nfn bar() {\n  c\n}";
        let symbols = vec![
            Symbol {
                kind: SymbolKind::Function,
                name: "foo".into(),
                qualified_name: "foo".into(),
                start_line: 3,
                end_line: 6,
                signature: Some("fn foo()".into()),
                docstring: None,
            },
            Symbol {
                kind: SymbolKind::Function,
                name: "bar".into(),
                qualified_name: "bar".into(),
                start_line: 8,
                end_line: 10,
                signature: Some("fn bar()".into()),
                docstring: None,
            },
        ];
        let chunks = split_code_by_symbols(content, &symbols, &cfg(), &ChunkMetadata::default());
        // 2 个符号块 + 1 个孤儿块（import/注释行）
        assert_eq!(chunks.len(), 3);
        // 按行号排序
        for w in chunks.windows(2) {
            assert!(w[0].metadata.line_start <= w[1].metadata.line_start);
        }
        let syms: Vec<_> = chunks
            .iter()
            .filter(|c| c.metadata.symbol_name.is_some())
            .collect();
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].metadata.symbol_name.as_deref(), Some("foo"));
        assert_eq!(syms[1].metadata.symbol_name.as_deref(), Some("bar"));
        // 孤儿块无符号信息
        let orphans: Vec<_> = chunks
            .iter()
            .filter(|c| c.metadata.symbol_name.is_none())
            .collect();
        assert_eq!(orphans.len(), 1);
    }

    #[test]
    fn test_split_code_by_symbols_empty_falls_back() {
        let content = "line1\nline2\nline3";
        let chunks = split_code_by_symbols(content, &[], &cfg(), &ChunkMetadata::default());
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_split_document_dispatches_markdown() {
        let doc = ParsedDocument {
            text: "# A\nbody".into(),
            file_type: "markdown".into(),
            language: None,
        };
        let chunks = split_document(&doc, &cfg());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].metadata.heading.as_deref(), Some("# A"));
    }

    fn mk_chunk(text: &str) -> Chunk {
        Chunk {
            content: text.to_string(),
            token_count: token_count(text),
            metadata: ChunkMetadata::default(),
        }
    }

    #[test]
    fn test_build_parents_groups_by_target() {
        // 每块 16 字符 ≈ 4 token，目标 16 token、上限 4 → 每父块 4 子块
        let chunks: Vec<Chunk> = (0..9)
            .map(|i| mk_chunk(&format!("{:04}", i).repeat(4)))
            .collect();
        let groups = build_parents(&chunks, 16, 4);
        assert_eq!(groups.len(), 3); // 4 + 4 + 1
        assert_eq!(groups[0].child_range, 0..4);
        assert_eq!(groups[1].child_range, 4..8);
        assert_eq!(groups[2].child_range, 8..9);
        // 父块正文以 \n\n 拼接其子块
        assert!(groups[0].content.contains("\n\n"));
    }

    #[test]
    fn test_build_parents_max_children() {
        // 每块 1 token，目标巨大 → 受 max_children 约束
        let chunks: Vec<Chunk> = (0..9).map(|i| mk_chunk(&format!("c{i}"))).collect();
        let groups = build_parents(&chunks, 1000, 3);
        assert_eq!(groups.len(), 3); // 3 + 3 + 3
        assert!(groups.iter().all(|g| g.child_range.len() <= 3));
    }

    #[test]
    fn test_build_parents_empty() {
        assert!(build_parents(&[], 16, 4).is_empty());
    }
}
