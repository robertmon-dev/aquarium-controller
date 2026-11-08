import type { ReactNode, HTMLAttributes } from "react";

export interface SidebarProps extends HTMLAttributes<HTMLElement> {
  children: ReactNode;
  isCollapsed?: boolean;
}

export interface SidebarSectionProps extends HTMLAttributes<
  HTMLDivElement | HTMLElement
> {
  children: ReactNode;
}
