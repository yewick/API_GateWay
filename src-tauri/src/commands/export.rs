//! 导出辅助命令：前端「另存为」选择路径后，由 Rust 侧落盘写文件。

/// 将文本内容写入指定路径（覆盖写）。用于日志 CSV/JSON 导出。
#[tauri::command]
pub fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败: {e}"))
}
