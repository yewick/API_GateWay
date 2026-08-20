#!/usr/bin/env python3
"""PDF -> JSON blocks 薄提取器（PyMuPDF）。

用法: python3 pymupdf_extract.py <pdf_path>
输出: stdout 一段 JSON 数组，每项为 block：
  {"page":int, "type":"text"|"image", "bbox":[x0,y0,x1,y1],
   "lines":[{"bbox":[x0,y0,x1,y1],
             "spans":[{"text":str,"size":float,"font":str,"flags":int,"bbox":[x0,y0,x1,y1]}]}]}

只做「PDF → 结构化块」，不做 Markdown；布局/字体/坐标分析在 Rust 侧完成。
异常写入 stderr 并以非零码退出。
"""
import json
import sys


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: pymupdf_extract.py <pdf_path>", file=sys.stderr)
        return 2

    try:
        import fitz  # PyMuPDF
    except ImportError:
        print("缺少依赖：请先 `pip install pymupdf`", file=sys.stderr)
        return 3

    pdf_path = sys.argv[1]
    try:
        doc = fitz.open(pdf_path)
    except Exception as exc:  # noqa: BLE001 - 统一转为非零退出
        print(f"打开 PDF 失败: {exc}", file=sys.stderr)
        return 4

    blocks = []
    for page in doc:
        pno = page.number + 1  # 1-indexed，与 Rust 侧一致
        data = page.get_text("dict")
        for blk in data.get("blocks", []):
            if blk.get("type") == 0:  # text
                lines = []
                for line in blk.get("lines", []):
                    spans = []
                    for sp in line.get("spans", []):
                        if not sp.get("text", "").strip():
                            continue
                        spans.append({
                            "text": sp.get("text", ""),
                            "size": round(sp.get("size", 0.0), 3),
                            "font": sp.get("font", ""),
                            "flags": sp.get("flags", 0),
                            "bbox": [round(v, 2) for v in sp.get("bbox", [0, 0, 0, 0])],
                        })
                    if spans:
                        lines.append({
                            "bbox": [round(v, 2) for v in line.get("bbox", [0, 0, 0, 0])],
                            "spans": spans,
                        })
                if lines:
                    blocks.append({
                        "page": pno,
                        "type": "text",
                        "bbox": [round(v, 2) for v in blk.get("bbox", [0, 0, 0, 0])],
                        "lines": lines,
                    })
            elif blk.get("type") == 1:  # image
                blocks.append({
                    "page": pno,
                    "type": "image",
                    "bbox": [round(v, 2) for v in blk.get("bbox", [0, 0, 0, 0])],
                    "lines": [],
                })

    doc.close()
    json.dump(blocks, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
