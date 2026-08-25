//! tree-sitter 符号感知代码解析。
//!
//! 通过 [`extract_symbols`] 把代码文件解析为符号列表（函数/类/方法/结构体等），
//! 供 [`super::splitter::split_code_by_symbols`] 按符号边界分块。当前覆盖
//! Rust / Python / TypeScript(TSX) / JavaScript(JSX) / Java 五种语言，其余语言
//! 返回空（回退到固定大小分块）。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

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

/// 一个代码符号（来自 tree-sitter AST）。
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
    /// 文档注释（当前未提取，恒为 None）
    pub docstring: Option<String>,
}

/// 递归遍历时传递的上下文。
#[derive(Debug, Clone, Default)]
struct Ctx {
    /// 外层类型名（类/结构体/impl 目标），用于生成限定名
    container: Option<String>,
    /// 是否处于类/接口等类型体内（其中的函数视为方法）
    in_class: bool,
    /// 是否处于函数体内（其中不再提取模块级变量）
    in_function: bool,
}

/// 提取源码中的符号。`language` 为 [`super::parser::determine_language`] 产出的
/// 语言名（如 `rust` / `python` / `typescript` / `javascript` / `java`）。
/// 未支持的语言返回空列表。
pub fn extract_symbols(language: &str, source: &str) -> Vec<Symbol> {
    let lang_fn = match language {
        "rust" => tree_sitter_rust::LANGUAGE,
        "python" => tree_sitter_python::LANGUAGE,
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX,
        "javascript" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE,
        "java" => tree_sitter_java::LANGUAGE,
        _ => return Vec::new(),
    };

    let mut parser = Parser::new();
    if let Err(e) = parser.set_language(&lang_fn.into()) {
        tracing::warn!("加载 {language} 语法失败: {e}");
        return Vec::new();
    }
    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut out = Vec::new();
    walk_node(
        tree.root_node(),
        source.as_bytes(),
        language,
        &Ctx::default(),
        &mut out,
    );
    out
}

/// 深度优先遍历 AST，逐节点分派。
fn walk_node(node: Node, source: &[u8], lang: &str, ctx: &Ctx, out: &mut Vec<Symbol>) {
    let child_ctx = dispatch(node, source, lang, ctx, out).unwrap_or_else(|| ctx.clone());
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        walk_node(child, source, lang, &child_ctx, out);
    }
}

/// 处理单个节点：可能产出符号，并返回其子节点的上下文（`None` 表示沿用父上下文）。
fn dispatch(node: Node, source: &[u8], lang: &str, ctx: &Ctx, out: &mut Vec<Symbol>) -> Option<Ctx> {
    let kind = node.kind();
    let name = field_name(&node, source);
    let start = node.start_position().row;
    let end = node.end_position().row;
    let src = std::str::from_utf8(source).unwrap_or("");
    let sig = line_at(src, start);

    // Java 字段声明可含多个变量声明器，逐个生成（按 final 区分常量/变量）
    if lang == "java" && kind == "field_declaration" {
        let fk = if has_modifier(&node, source, "final") {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };
        let mut cursor = node.walk();
        for c in node.named_children(&mut cursor) {
            if c.kind() == "variable_declarator" {
                if let Some(n) = field_name(&c, source) {
                    out.push(make_symbol(fk, n.clone(), n, start, end, Some(sig.clone())));
                }
            }
        }
        return None;
    }

    match (lang, kind) {
        // ---------- Rust ----------
        ("rust", "function_item") => {
            push_fn_like(out, ctx, name, start, end, sig);
            return Some(Ctx {
                in_function: true,
                ..Default::default()
            });
        }
        ("rust", "struct_item") => return Some(push_class_like(out, SymbolKind::Struct, name, start, end, sig)),
        ("rust", "enum_item") => return Some(push_class_like(out, SymbolKind::Enum, name, start, end, sig)),
        ("rust", "trait_item") => return Some(push_class_like(out, SymbolKind::Interface, name, start, end, sig)),
        ("rust", "impl_item") => {
            // impl 块本身不产出符号，但内部函数视为方法（限定到 impl 目标类型）
            let tname = node
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string());
            return Some(Ctx {
                container: tname,
                in_class: true,
                in_function: false,
            });
        }
        ("rust", "type_item") => {
            let nm = name.clone().unwrap_or_default();
            out.push(make_symbol(SymbolKind::TypeAlias, nm.clone(), nm, start, end, Some(sig)));
        }
        ("rust", "const_item") => {
            let nm = name.clone().unwrap_or_default();
            out.push(make_symbol(SymbolKind::Constant, nm.clone(), nm, start, end, Some(sig)));
        }
        ("rust", "mod_item") => {
            let nm = name.clone().unwrap_or_default();
            out.push(make_symbol(SymbolKind::Namespace, nm.clone(), nm, start, end, Some(sig)));
            return Some(Ctx {
                container: name.clone(),
                in_class: false,
                in_function: false,
            });
        }

        // ---------- Python ----------
        ("python", "function_definition") => {
            push_fn_like(out, ctx, name, start, end, sig);
            return Some(Ctx {
                in_function: true,
                ..Default::default()
            });
        }
        ("python", "class_definition") => return Some(push_class_like(out, SymbolKind::Class, name, start, end, sig)),

        // ---------- TypeScript / JavaScript ----------
        ("typescript", "function_declaration") | ("javascript", "function_declaration") => {
            push_fn_like(out, ctx, name, start, end, sig);
            return Some(Ctx {
                in_function: true,
                ..Default::default()
            });
        }
        ("typescript", "class_declaration") | ("javascript", "class_declaration") => {
            return Some(push_class_like(out, SymbolKind::Class, name, start, end, sig));
        }
        ("typescript", "method_definition") | ("javascript", "method_definition") => {
            push_fn_like(out, ctx, name, start, end, sig);
            return Some(Ctx {
                in_function: true,
                ..Default::default()
            });
        }
        ("typescript", "interface_declaration") => {
            return Some(push_class_like(out, SymbolKind::Interface, name, start, end, sig));
        }
        ("typescript", "enum_declaration") => {
            return Some(push_class_like(out, SymbolKind::Enum, name, start, end, sig));
        }
        ("typescript", "type_alias_declaration") => {
            let nm = name.clone().unwrap_or_default();
            out.push(make_symbol(SymbolKind::TypeAlias, nm.clone(), nm, start, end, Some(sig)));
        }
        ("typescript", "variable_declarator") | ("javascript", "variable_declarator") => {
            // 仅提取模块级变量，跳过类字段与函数内局部变量
            if !ctx.in_class && !ctx.in_function {
                let nm = name.clone().unwrap_or_default();
                out.push(make_symbol(SymbolKind::Variable, nm.clone(), nm, start, end, Some(sig)));
            }
        }

        // ---------- Java ----------
        ("java", "class_declaration") => return Some(push_class_like(out, SymbolKind::Class, name, start, end, sig)),
        ("java", "interface_declaration") => return Some(push_class_like(out, SymbolKind::Interface, name, start, end, sig)),
        ("java", "enum_declaration") => return Some(push_class_like(out, SymbolKind::Enum, name, start, end, sig)),
        ("java", "method_declaration") => {
            push_fn_like(out, ctx, name, start, end, sig);
            return Some(Ctx {
                in_function: true,
                ..Default::default()
            });
        }
        ("java", "constructor_declaration") => {
            let nm = name.clone().or_else(|| ctx.container.clone()).unwrap_or_default();
            let q = qualify(ctx.container.as_deref(), Some(nm.as_str()));
            out.push(make_symbol(SymbolKind::Method, nm, q, start, end, Some(sig)));
            return Some(Ctx {
                in_function: true,
                ..Default::default()
            });
        }

        _ => {}
    }
    None
}

/// 生成函数/方法符号（依上下文判断 Function 还是 Method），并返回「函数体」上下文。
fn push_fn_like(
    out: &mut Vec<Symbol>,
    ctx: &Ctx,
    name: Option<String>,
    start: usize,
    end: usize,
    sig: String,
) {
    let nm = name.clone().unwrap_or_default();
    let (k, q) = if ctx.in_class {
        (
            SymbolKind::Method,
            qualify(ctx.container.as_deref(), name.as_deref()),
        )
    } else {
        (SymbolKind::Function, nm.clone())
    };
    out.push(make_symbol(k, nm, q, start, end, Some(sig)));
}

/// 生成类型类符号（类/结构体/枚举/接口），返回「类型体」上下文（内部函数视为方法）。
fn push_class_like(
    out: &mut Vec<Symbol>,
    kind: SymbolKind,
    name: Option<String>,
    start: usize,
    end: usize,
    sig: String,
) -> Ctx {
    let nm = name.clone().unwrap_or_default();
    out.push(make_symbol(kind, nm.clone(), nm.clone(), start, end, Some(sig)));
    Ctx {
        container: name,
        in_class: true,
        in_function: false,
    }
}

fn make_symbol(
    kind: SymbolKind,
    name: String,
    qualified_name: String,
    start_line: usize,
    end_line: usize,
    signature: Option<String>,
) -> Symbol {
    Symbol {
        kind,
        name,
        qualified_name,
        start_line,
        end_line,
        signature,
        docstring: None,
    }
}

/// 限定名：`container.name`；缺容器时退化为 `name`。
fn qualify(container: Option<&str>, name: Option<&str>) -> String {
    match (container, name) {
        (Some(c), Some(n)) => format!("{c}.{n}"),
        (_, Some(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// 取节点的 `name` 字段文本。
fn field_name(node: &Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
}

/// 判断节点是否带某修饰符（如 Java 的 `final`）。
fn has_modifier(node: &Node, source: &[u8], modifier: &str) -> bool {
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        if c.kind() == "modifiers" {
            if let Ok(t) = c.utf8_text(source) {
                return t.split_whitespace().any(|w| w == modifier);
            }
        }
    }
    false
}

/// 取源码第 `row` 行（trim 后），用于函数签名。
fn line_at(source: &str, row: usize) -> String {
    source
        .lines()
        .nth(row)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_functions_and_structs() {
        let src = "fn foo() {}\nstruct Bar { x: i32 }\nimpl Bar {\n  fn method(&self) {}\n}\n";
        let syms = extract_symbols("rust", src);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"Bar"));
        assert!(names.contains(&"method"));
        let m = syms.iter().find(|s| s.name == "method").unwrap();
        assert_eq!(m.kind, SymbolKind::Method);
        assert!(m.qualified_name.contains("Bar"));
    }

    #[test]
    fn test_python_functions_and_classes() {
        let src = "def top():\n    pass\n\nclass A:\n    def m(self):\n        pass\n";
        let syms = extract_symbols("python", src);
        let top = syms.iter().find(|s| s.name == "top").unwrap();
        assert_eq!(top.kind, SymbolKind::Function);
        assert_eq!(top.start_line, 0);
        let a = syms.iter().find(|s| s.name == "A").unwrap();
        assert_eq!(a.kind, SymbolKind::Class);
        let m = syms.iter().find(|s| s.name == "m").unwrap();
        assert_eq!(m.kind, SymbolKind::Method);
        assert!(m.qualified_name.contains("A"));
    }

    #[test]
    fn test_java_class_and_method() {
        let src = "public class Foo {\n  public void bar() {}\n}\n";
        let syms = extract_symbols("java", src);
        assert!(syms.iter().any(|s| s.name == "Foo" && s.kind == SymbolKind::Class));
        assert!(syms.iter().any(|s| s.name == "bar" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_typescript_class_and_interface() {
        let src = "interface I {}\nclass C implements I {\n  m() {}\n}\n";
        let syms = extract_symbols("typescript", src);
        assert!(syms.iter().any(|s| s.name == "I" && s.kind == SymbolKind::Interface));
        assert!(syms.iter().any(|s| s.name == "C" && s.kind == SymbolKind::Class));
        assert!(syms.iter().any(|s| s.name == "m" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_unsupported_language_returns_empty() {
        assert!(extract_symbols("go", "func main() {}").is_empty());
    }
}
