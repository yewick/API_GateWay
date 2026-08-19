//! tree-sitter 符号感知代码解析。
//!
//! 本次仅落地 `Symbol`/`SymbolKind` 纯数据类型，供 [`super::splitter::split_code_by_symbols`]
//! 使用。真正的 AST 提取（`get_language` / `extract_symbols` / `walk_node` / 各语言
//! `check_*_node`）需要引入 tree-sitter 相关 grammar crate 并逐语言映射节点类型，
//! 方案尚未明确，留待下次讨论后实现。届时代码文件的分块即可从「固定大小」升级为
//! 「按函数/类/方法边界」。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 代码符号类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Enum,
    Variable,
    Constant,
    TypeAlias,
    Namespace,
}

impl SymbolKind {
    /// 转小写字符串，便于写入 `kb_chunks.symbol_kind` 与 `metadata`。
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Interface => "interface",
            SymbolKind::Enum => "enum",
            SymbolKind::Variable => "variable",
            SymbolKind::Constant => "constant",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Namespace => "namespace",
        }
    }
}

/// 一个代码符号（来自 tree-sitter AST，本次由未来实现填充）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub kind: SymbolKind,
    /// 符号名
    pub name: String,
    /// 限定名（如 `Class.method`）
    pub qualified_name: String,
    /// 开始行（0-indexed）
    pub start_line: usize,
    /// 结束行（0-indexed, inclusive）
    pub end_line: usize,
    /// 函数签名（第一行）
    pub signature: Option<String>,
    /// 文档注释（如 Python docstring）
    pub docstring: Option<String>,
}
