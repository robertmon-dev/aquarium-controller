import type { ReactNode, HTMLAttributes } from "react";

export interface NavbarProps extends HTMLAttributes<HTMLElement> {
  children: ReactNode;
}

export interface NavbarSectionProps extends HTMLAttributes<
  HTMLDivElement | HTMLElement
> {
  children: ReactNode;
}
