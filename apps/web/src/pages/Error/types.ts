import type { TranslationKey } from "@aquarium/i18n";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

export type ErrorCode =
  | "not-configured"
  | "offline"
  | "unauthorized"
  | "not-found"
  | "device-error";

export interface ErrorConfigValue {
  icon: LucideIcon;
  badge: string;
  titleKey: TranslationKey;
  descKey: TranslationKey;
}

export interface ErrorPageProps {
  code: ErrorCode;
  title?: string;
  message?: string;
  action?: ReactNode;
  className?: string;
}
