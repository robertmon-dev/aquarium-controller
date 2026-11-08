import type { ReactNode } from "react";

export type ModalSize =
  | "default"
  | "1vw"
  | "5vw"
  | "10vw"
  | "20vw"
  | "40vw"
  | "60vw"
  | "90vw"
  | "95vw";

export interface ModalProps {
  open: boolean;
  title: string;
  children: ReactNode;
  onClose?: () => void;
  size?: ModalSize;
  role?: "dialog" | "alertdialog";
  ariaDescribedById?: string;
  titleId?: string;
  className?: string;
}
