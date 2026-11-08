import {
  Ghost,
  PlugZap,
  ServerCrash,
  ShieldAlert,
  WifiOff,
} from "lucide-react";
import type { ErrorCode, ErrorConfigValue } from "./types";

export const ERROR_CONFIG: Record<ErrorCode, ErrorConfigValue> = {
  "not-configured": {
    icon: PlugZap,
    badge: "—",
    titleKey: "errors.notConfigured.title",
    descKey: "errors.notConfigured.description",
  },
  offline: {
    icon: WifiOff,
    badge: "ERR",
    titleKey: "errors.offline.title",
    descKey: "errors.offline.description",
  },
  unauthorized: {
    icon: ShieldAlert,
    badge: "401",
    titleKey: "errors.unauthorized.title",
    descKey: "errors.unauthorized.description",
  },
  "not-found": {
    icon: Ghost,
    badge: "404",
    titleKey: "errors.notFound.title",
    descKey: "errors.notFound.description",
  },
  "device-error": {
    icon: ServerCrash,
    badge: "500",
    titleKey: "errors.deviceError.title",
    descKey: "errors.deviceError.description",
  },
};
