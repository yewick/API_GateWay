#!/usr/bin/env bash
# 验证 Parent/Child 检索：迁移是否生效、父子块是否生成、子块是否正确挂到父块。
#
# 用法：
#   ./verify_parent_child.sh                 # 检查 schema + 最近入库的文档
#   ./verify_parent_child.sh <doc_id>        # 指定文档
#   ./verify_parent_child.sh --all           # 全局统计所有文档
#   DB=/path/to/yeapi.db ./verify_parent_child.sh   # 覆盖数据库路径
#
# 注意：父子块关系只在「入库」那一刻生成，之前入库的旧文档 chunk 的 parent_id 恒为 NULL，
#       需重新入库后才能看到关联。

set -euo pipefail

DB="${DB:-$HOME/Library/Application Support/com.YeAPI.app/yeapi.db}"

die() { echo "✗ $*" >&2; exit 1; }
ok()  { echo "✓ $*"; }

command -v sqlite3 >/dev/null || die "未找到 sqlite3（macOS 自带；或 brew install sqlite）"
[ -f "$DB" ] || die "数据库不存在: $DB"

echo "== DB: $DB =="

# ---------------------------------------------------------------------------
# 1. schema
# ---------------------------------------------------------------------------
echo
echo "── 1. 迁移 018 是否已应用 ──"
TABLE_OK="$(sqlite3 "$DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kb_chunk_parents';")"
COL_OK="$(sqlite3 "$DB" "SELECT COUNT(*) FROM pragma_table_info('kb_chunks') WHERE name='parent_id';")"
[ "$TABLE_OK" = "1" ] && ok "kb_chunk_parents 表存在" || die "kb_chunk_parents 表缺失（迁移 018 未应用？先 npm run tauri dev 启动一次）"
[ "$COL_OK" = "1" ] && ok "kb_chunks.parent_id 列存在" || die "kb_chunks 缺少 parent_id 列"

# ---------------------------------------------------------------------------
# 2. 全局统计
# ---------------------------------------------------------------------------
echo
echo "── 2. 全局父子块统计 ──"
sqlite3 -header -column "$DB" "
SELECT COUNT(*)                    AS 总子块,
       COALESCE(SUM(parent_id IS NULL), 0)     AS 无父块_旧数据,
       COALESCE(SUM(parent_id IS NOT NULL), 0) AS 已挂父块
FROM kb_chunks;"
sqlite3 -header -column "$DB" "
SELECT (SELECT COUNT(*) FROM kb_chunk_parents)          AS 总父块,
       (SELECT COUNT(DISTINCT doc_id) FROM kb_chunk_parents) AS 涉及文档数;"

if [ "${1:-}" = "--all" ]; then
  echo
  echo "── 2b. 各文档父子块明细（仅 ready）──"
  sqlite3 -header -column "$DB" "
  SELECT substr(d.filename,1,32)                          AS 文档,
         d.chunk_count                                    AS 子块,
         COUNT(p.id)                                      AS 父块,
         COALESCE(SUM(c.parent_id IS NULL), 0)            AS 无父块子块
  FROM kb_documents d
  LEFT JOIN kb_chunk_parents p ON p.doc_id = d.id
  LEFT JOIN kb_chunks c       ON c.doc_id = d.id
  WHERE d.status = 'ready'
  GROUP BY d.id ORDER BY d.created_at DESC;"
  exit 0
fi

DOC="${1:-}"

# ---------------------------------------------------------------------------
# 3. 选定文档
# ---------------------------------------------------------------------------
if [ -z "$DOC" ]; then
  echo
  echo "── 3. 最近入库的文档（默认选第一个 ready）──"
  sqlite3 -header -column "$DB" "
  SELECT id, substr(filename,1,40) AS filename, chunk_count, status
  FROM kb_documents ORDER BY created_at DESC LIMIT 10;"
  DOC="$(sqlite3 "$DB" "SELECT id FROM kb_documents WHERE status='ready' ORDER BY created_at DESC LIMIT 1;")"
  [ -n "$DOC" ] || die "没有 ready 状态的文档，请先在 UI 里「入库」一份文档"
  echo "→ 默认选用: $DOC"
fi

# ---------------------------------------------------------------------------
# 4. 文档级明细
# ---------------------------------------------------------------------------
echo
echo "── 4. 文档 $DOC 的父子块 ──"
sqlite3 -header -column "$DB" "
SELECT d.filename,
       d.chunk_count                                                     AS 子块数,
       (SELECT COUNT(*) FROM kb_chunk_parents WHERE doc_id=d.id)         AS 父块数,
       (SELECT COUNT(*) FROM kb_chunks WHERE doc_id=d.id AND parent_id IS NOT NULL) AS 已挂父块,
       (SELECT COUNT(*) FROM kb_chunks WHERE doc_id=d.id AND parent_id IS NULL)     AS 无父块
FROM kb_documents d WHERE d.id='$DOC';"

echo
echo "── 4b. 每个父块 → 其子块（子块下标区间）──"
sqlite3 -header -column "$DB" "
SELECT p.id,
       p.chunk_index                  AS 父块序号,
       p.token_count                  AS tokens,
       substr(replace(p.content, char(10), ' '),1,48) AS 正文预览,
       COUNT(c.id)                    AS 子块数,
       GROUP_CONCAT(c.chunk_index)    AS 子块下标
FROM kb_chunk_parents p
LEFT JOIN kb_chunks c ON c.parent_id = p.id
WHERE p.doc_id='$DOC'
GROUP BY p.id ORDER BY p.chunk_index;"

# ---------------------------------------------------------------------------
# 5. 完整性判定
# ---------------------------------------------------------------------------
echo
echo "── 5. 完整性 ──"
PARENT_CNT="$(sqlite3 "$DB" "SELECT COUNT(*) FROM kb_chunk_parents WHERE doc_id='$DOC';")"
LINK_CNT="$(sqlite3 "$DB" "SELECT COUNT(*) FROM kb_chunks WHERE doc_id='$DOC' AND parent_id IS NOT NULL;")"
CHILD_CNT="$(sqlite3 "$DB" "SELECT COUNT(*) FROM kb_chunks WHERE doc_id='$DOC';")"
ORPHAN="$(sqlite3 "$DB" "SELECT COUNT(*) FROM kb_chunk_parents p WHERE p.doc_id='$DOC' AND NOT EXISTS (SELECT 1 FROM kb_chunks c WHERE c.parent_id=p.id);")"

[ "$ORPHAN" = "0" ] && ok "无孤儿父块（每个父块都被子块引用）" || echo "⚠ 有 $ORPHAN 个父块无子块引用"

if [ "$PARENT_CNT" -gt 0 ] && [ "$LINK_CNT" -eq "$CHILD_CNT" ] && [ "$ORPHAN" = "0" ]; then
  echo
  ok "PASS：父块 $PARENT_CNT 个，全部 $CHILD_CNT 个子块均已挂到父块，无孤儿。"
  echo "    （此文档已正确生成父子块；问答时命中子块将用父块正文补全上下文。）"
else
  echo
  if [ "$PARENT_CNT" = "0" ]; then
    echo "⚠ 该文档 0 个父块：可能尚未重新入库（旧数据无父块）。请删除后重新上传并入库。"
  else
    echo "⚠ 关联不完整：子块 $CHILD_CNT 个，其中仅 $LINK_CNT 个挂了父块（其余为旧数据，需重灌）。"
  fi
fi
