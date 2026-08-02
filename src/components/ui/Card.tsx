import type { ReactNode } from "react";

interface CardProps {
  title?: string;
  description?: string;
  headerRight?: ReactNode;
  children?: ReactNode;
  className?: string;
  noPadding?: boolean;
}

export function Card({
  title,
  description,
  headerRight,
  children,
  className = "",
  noPadding = false,
}: CardProps) {
  return (
    <div
      className={`bg-bg-secondary border border-border-primary rounded-xl shadow-sm ${
        noPadding ? "" : "p-5"
      } ${className}`}
    >
      {(title || headerRight) && (
        <div className="flex items-center justify-between mb-4">
          <div>
            {title && (
              <h3 className="text-sm font-semibold text-text-primary">{title}</h3>
            )}
            {description && (
              <p className="text-xs text-text-secondary mt-0.5">{description}</p>
            )}
          </div>
          {headerRight && <div className="flex items-center gap-2">{headerRight}</div>}
        </div>
      )}
      {children}
    </div>
  );
}
