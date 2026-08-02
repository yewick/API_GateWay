export interface TabItem {
  key: string;
  label: string;
  icon?: React.ReactNode;
  badge?: number;
}

interface TabsProps {
  tabs: TabItem[];
  activeKey: string;
  onChange: (key: string) => void;
  className?: string;
}

export function Tabs({ tabs, activeKey, onChange, className = "" }: TabsProps) {
  return (
    <div
      className={`flex gap-1 border-b border-border-primary ${className}`}
      role="tablist"
    >
      {tabs.map((tab) => {
        const active = tab.key === activeKey;
        return (
          <button
            key={tab.key}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(tab.key)}
            className={`relative px-4 py-2.5 text-sm font-medium rounded-t-lg transition-colors flex items-center gap-1.5 ${
              active
                ? "text-accent"
                : "text-text-secondary hover:text-text-primary hover:bg-bg-hover/50"
            }`}
          >
            {tab.icon}
            {tab.label}
            {typeof tab.badge === "number" && tab.badge > 0 && (
              <span className="ml-0.5 px-1.5 py-0.5 text-[10px] leading-none rounded-full bg-accent/15 text-accent">
                {tab.badge}
              </span>
            )}
            {active && (
              <span className="absolute inset-x-2 -bottom-px h-0.5 bg-accent rounded-full" />
            )}
          </button>
        );
      })}
    </div>
  );
}
