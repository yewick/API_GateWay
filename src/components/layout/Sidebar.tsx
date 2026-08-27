import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  BarChart3,
  Network,
  KeyRound,
  ScrollText,
  Settings,
  BookOpen,
  Plug,
  Zap,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { NAV_ITEMS, type NavItem } from "../../lib/constants";
import { ThemeToggle } from "../common/ThemeToggle";
import { UpdateChecker } from "../common/UpdateChecker";
import { useSidebarStore } from "../../stores/uiStore";

const iconMap: Record<
  string,
  React.ComponentType<{ size?: number; className?: string }>
> = {
  LayoutDashboard,
  BarChart3,
  Network,
  KeyRound,
  ScrollText,
  Settings,
  BookOpen,
  Plug,
};

export function Sidebar() {
  const collapsed = useSidebarStore((s) => s.collapsed);
  const toggle = useSidebarStore((s) => s.toggle);

  return (
    <aside
      className={`flex flex-col h-full bg-bg-secondary border-r border-border-primary transition-all duration-200 ${
        collapsed ? "w-[60px]" : "w-[230px]"
      }`}
    >
      {/* Logo 区域 */}
      <div
        className={`flex items-center h-14 px-4 border-b border-border-primary ${
          collapsed ? "justify-center" : "gap-2.5"
        }`}
      >
        <div className="w-8 h-8 rounded-lg bg-accent/15 flex items-center justify-center flex-shrink-0">
          <Zap size={18} className="text-accent" />
        </div>
        {!collapsed && (
          <div className="leading-tight">
            <div className="text-sm font-bold text-text-primary">YeAPI</div>
            <div className="text-[10px] text-text-muted">LLM API Gateway</div>
          </div>
        )}
      </div>

      {/* 折叠按钮 */}
      <button
        onClick={toggle}
        className="m-2 p-1.5 rounded-md text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors self-start"
        aria-label={collapsed ? "展开侧边栏" : "折叠侧边栏"}
        title={collapsed ? "展开侧边栏" : "折叠侧边栏"}
      >
        {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
      </button>

      {/* 导航菜单 */}
      <nav className="flex-1 px-2 overflow-y-auto">
        <ul className="space-y-0.5">
          {NAV_ITEMS.map((item: NavItem) => (
            <SidebarNavItem
              key={item.path}
              item={item}
              collapsed={collapsed}
            />
          ))}
        </ul>
      </nav>

      {/* 底部：主题切换 + 检查更新 + 版本 */}
      <div className="border-t border-border-primary p-2 space-y-1">
        <div className={collapsed ? "flex justify-center" : ""}>
          <ThemeToggle className="w-full justify-start" />
        </div>
        <div className={collapsed ? "flex justify-center" : ""}>
          <UpdateChecker collapsed={collapsed} />
        </div>
        {!collapsed && (
          <p className="px-3 py-1 text-[10px] text-text-muted">YeAPI v0.1.0</p>
        )}
      </div>
    </aside>
  );
}

function SidebarNavItem({
  item,
  collapsed,
}: {
  item: NavItem;
  collapsed: boolean;
}) {
  const Icon = iconMap[item.icon];
  return (
    <li>
      <NavLink
        to={item.path}
        end={item.path === "/"}
        title={collapsed ? item.label : undefined}
        className={({ isActive }) =>
          `relative flex items-center rounded-lg transition-colors ${
            collapsed ? "justify-center px-0 py-2.5" : "px-3 py-2.5 gap-3"
          } ${
            isActive
              ? "text-accent bg-accent/10"
              : "text-text-secondary hover:text-text-primary hover:bg-bg-hover"
          }`
        }
      >
        {({ isActive }) => (
          <>
            {isActive && (
              <span className="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-5 bg-accent rounded-r-full" />
            )}
            {Icon && <Icon size={18} className="flex-shrink-0" />}
            {!collapsed && (
              <span className="text-sm font-medium">{item.label}</span>
            )}
          </>
        )}
      </NavLink>
    </li>
  );
}
