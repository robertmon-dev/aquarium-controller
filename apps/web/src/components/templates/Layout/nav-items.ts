import type { Translate } from "@aquarium/i18n";

export interface NavItem {
  to: string;
  label: string;
}

export function navItems(t: Translate): NavItem[] {
  return [
    { to: "/", label: t("nav.dashboard") },
    { to: "/lighting", label: t("nav.lighting") },
    { to: "/history", label: t("nav.history") },
    { to: "/settings", label: t("nav.settings") },
  ];
}
