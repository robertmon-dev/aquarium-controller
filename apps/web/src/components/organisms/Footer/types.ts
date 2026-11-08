import type { HTMLAttributes } from "react";
import type { ReactNode } from "react";

export interface FooterProps extends HTMLAttributes<HTMLElement> {
  brand?: ReactNode;
  copyright?: string;
  children?: ReactNode;
}
