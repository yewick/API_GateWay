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

/// 通用文本分块：按行累积 token，超限 flush，保留重叠内容。
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

/// Markdown 分块：按标题切段，每段一个 chunk；chunk 正文前携带完整标题路径
/// （祖先标题 + 当前标题）作上下文；纯标题段（无正文）跳过；超大段落递归 split_text。
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

        // 完整标题路径作为前缀，保证每个 chunk 自含层级上下文
        let header: String = stack
            .iter()
            .map(|(lv, t)| format!("{} {}", "#".repeat(*lv), t))
            .collect::<Vec<_>>()
            .join("\n");

        let full = format!("{header}\n{body}");
        if full.chars().count() <= max_section_chars {
            chunks.push(Chunk {
                token_count: token_count(&full),
                content: full,
                metadata: ChunkMetadata {
                    heading: Some(heading_text.clone()),
                    line_start: start,
                    line_end: body_end.saturating_sub(1),
                    ..metadata.clone()
                },
            });
        } else {
            // 超大段落：递归 split_text，完整标题路径作前缀
            for c in split_text(&body, config, metadata) {
                let content = format!("{header}\n{}", c.content);
                chunks.push(Chunk {
                    token_count: token_count(&content),
                    content,
                    metadata: ChunkMetadata {
                        heading: Some(heading_text.clone()),
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
    fn test_split_markdown_by_heading() {
        let content = "# 标题一\n内容一\n内容二\n\n## 标题二\n更多内容";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].metadata.heading.as_deref(), Some("标题一"));
        assert_eq!(chunks[1].metadata.heading.as_deref(), Some("标题二"));
    }

    #[test]
    fn test_split_markdown_nested_skips_title_only() {
        // 父标题无正文（紧接子标题）→ 不再产出「只有标题没有描述」的 chunk
        let content = "# 父标题\n## 子标题\n正文内容";
        let chunks = split_markdown(content, &cfg(), &ChunkMetadata::default());
        assert_eq!(chunks.len(), 1, "实际 {:?}", chunks.len());
        assert_eq!(chunks[0].metadata.heading.as_deref(), Some("子标题"));
        // 叶子 chunk 携带祖先标题上下文
        assert!(chunks[0].content.contains("# 父标题"));
        assert!(chunks[0].content.contains("## 子标题"));
        assert!(chunks[0].content.contains("正文内容"));
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
        assert_eq!(chunks[1].metadata.heading.as_deref(), Some("标题"));
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
        assert_eq!(chunks[0].metadata.heading.as_deref(), Some("A"));
    }
}
