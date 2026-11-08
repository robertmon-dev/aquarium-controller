import type { SidebarProps } from "./types";
import "./SideBar.scss";

export const Sidebar = ({
  children,
  className = "",
  isCollapsed = false,
  ...rest
}: SidebarProps) => {
  return (
    <aside
      className={`sidebar ${isCollapsed ? "sidebar--collapsed" : ""} ${className}`.trim()}
      {...rest}
    >
      <div className="sidebar__container">{children}</div>
    </aside>
  );
};
