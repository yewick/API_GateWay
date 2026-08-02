import type { ReactNode } from "react";
import { Spinner } from "./Spinner";
import { EmptyState } from "./EmptyState";

export interface Column<T> {
  key: string;
  title: string;
  render?: (value: unknown, record: T) => ReactNode;
  width?: string;
  align?: "left" | "center" | "right";
}

interface TableProps<T> {
  columns: Column<T>[];
  data: T[];
  rowKey?: (record: T) => string;
  onRowClick?: (record: T) => void;
  loading?: boolean;
  emptyText?: string;
  emptyDescription?: string;
  compact?: boolean;
}

export function Table<T>({
  columns,
  data,
  rowKey,
  onRowClick,
  loading = false,
  emptyText = "暂无数据",
  emptyDescription,
  compact = false,
}: TableProps<T>) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-16">
        <Spinner />
      </div>
    );
  }

  if (data.length === 0) {
    return <EmptyState title={emptyText} description={emptyDescription} />;
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-border-primary">
            {columns.map((col) => (
              <th
                key={col.key}
                style={{ width: col.width }}
                className={`px-4 ${compact ? "py-2" : "py-3"} text-xs font-medium text-text-muted uppercase tracking-wide ${
                  col.align === "right"
                    ? "text-right"
                    : col.align === "center"
                      ? "text-center"
                      : "text-left"
                }`}
              >
                {col.title}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((record) => {
            const recordAny = record as Record<string, unknown>;
            const key = rowKey
              ? rowKey(record)
              : String(recordAny.id ?? "");
            return (
              <tr
                key={key}
                onClick={onRowClick ? () => onRowClick(record) : undefined}
                className={`border-b border-border-primary/60 transition-colors ${
                  onRowClick ? "cursor-pointer hover:bg-bg-hover/50" : "hover:bg-bg-hover/50"
                }`}
              >
                {columns.map((col) => (
                  <td
                    key={col.key}
                    className={`px-4 ${compact ? "py-2" : "py-3"} ${
                      col.align === "right"
                        ? "text-right"
                        : col.align === "center"
                          ? "text-center"
                          : "text-left"
                    }`}
                  >
                    {col.render
                      ? col.render(recordAny[col.key], record)
                      : String(recordAny[col.key] ?? "-")}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
