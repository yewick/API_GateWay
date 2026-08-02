import { createPortal } from "react-dom";
import { CheckCircle2, AlertCircle, AlertTriangle, Info, X } from "lucide-react";
import { useToastStore, type ToastItem, type ToastVariant } from "../../lib/toast";

const variantConfig: Record<
  ToastVariant,
  { icon: React.ReactNode; colorClass: string }
> = {
  success: { icon: <CheckCircle2 size={18} />, colorClass: "text-success" },
  error: { icon: <AlertCircle size={18} />, colorClass: "text-danger" },
  warning: { icon: <AlertTriangle size={18} />, colorClass: "text-warning" },
  info: { icon: <Info size={18} />, colorClass: "text-info" },
};

function ToastCard({ item }: { item: ToastItem }) {
  const dismiss = useToastStore((s) => s.dismiss);
  const { icon, colorClass } = variantConfig[item.variant];

  return (
    <div
      className="w-80 bg-bg-secondary border border-border-primary rounded-lg shadow-lg p-3.5 flex items-start gap-3 animate-toast-in"
      role="status"
    >
      <span className={`mt-0.5 flex-shrink-0 ${colorClass}`}>{icon}</span>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-text-primary">{item.title}</p>
        {item.description && (
          <p className="text-xs text-text-secondary mt-0.5 break-words">
            {item.description}
          </p>
        )}
      </div>
      <button
        onClick={() => dismiss(item.id)}
        className="flex-shrink-0 p-0.5 text-text-muted hover:text-text-primary transition-colors"
        aria-label="关闭通知"
      >
        <X size={14} />
      </button>
    </div>
  );
}

export function ToastProvider() {
  const toasts = useToastStore((s) => s.toasts);

  return createPortal(
    <div className="fixed bottom-5 right-5 z-[100] flex flex-col gap-2.5">
      {toasts.map((t) => (
        <ToastCard key={t.id} item={t} />
      ))}
    </div>,
    document.body,
  );
}

export { useToastStore };
