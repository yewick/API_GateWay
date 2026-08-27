import type { ReactNode } from "react";

interface TooltipProps {
  content: ReactNode;
  children: ReactNode;
  position?: "top" | "bottom";
  className?: string;
}

const positionClasses: Record<string, string> = {
  bottom: "top-full left-1/2 -translate-x-1/2 mt-1.5",
  top: "bottom-full left-1/2 -translate-x-1/2 mb-1.5",
};

export function Tooltip({ content, children, position = "bottom", className = "" }: TooltipProps) {
  return (
    <div className={`group relative inline-block max-w-full ${className}`}>
      {children}
      <div
        className={`absolute z-50 ${positionClasses[position]} opacity-0 group-hover:opacity-100 transition-opacity duration-150 pointer-events-none`}
      >
        <div className="px-3 py-2 bg-bg-tertiary border border-border-primary rounded-lg shadow-lg text-xs text-text-primary max-w-xs break-all whitespace-normal">
          {content}
        </div>
      </div>
    </div>
  );
}
