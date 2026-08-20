-- 文档表新增 content 列：存储解析后的完整文本（md/pdf → Markdown，txt → 纯文本，代码 → 源码）
-- 与 kb_chunks.content（分块片段）区分，供无损导出 / 展示 / 未来重新分块使用。
ALTER TABLE kb_documents ADD COLUMN content TEXT NOT NULL DEFAULT '';
