import type { ReactNode } from "react";

export function InfoRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] text-text-muted">{label}</div>
      <div
        className={`text-xs text-text-primary truncate ${mono ? "mono" : ""}`}
        title={value}
      >
        {value}
      </div>
    </div>
  );
}

export function Notice({
  tone,
  children,
}: {
  tone: "info" | "warning" | "danger" | "success";
  children: ReactNode;
}) {
  const classes = {
    info: "bg-info/10 border-info/30 text-info",
    warning: "bg-warning/10 border-warning/30 text-warning",
    danger: "bg-danger/10 border-danger/30 text-danger",
    success: "bg-success/10 border-success/30 text-success",
  }[tone];
  return (
    <div className={`rounded-lg border px-3 py-2 text-xs leading-relaxed ${classes}`}>
      {children}
    </div>
  );
}
