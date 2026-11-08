import { useT } from "@aquarium/i18n/use-locale";
import { streamBadge } from "@aquarium/view-model";
import { Activity, Gauge, Lightbulb, Settings } from "lucide-react";
import type { ReactNode } from "react";
import { NavLink } from "react-router-dom";
import { Badge } from "@/components/atoms";
import { Footer } from "@/components/organisms/Footer/Footer";
import { Navbar } from "@/components/organisms/NavBar/NavBar";
import { Sidebar } from "@/components/organisms/SideBar/SideBar";
import { useTelemetryStream } from "@/device/use-telemetry-stream";
import { useDevice } from "@/providers/AppProviders";
import { navItems } from "./nav-items";
import "./Layout.scss";

const ICONS = [Gauge, Lightbulb, Activity, Settings];

export function Layout({ children }: { children: ReactNode }) {
  const t = useT();
  const { transport } = useDevice();
  const stream = useTelemetryStream(transport);
  const badge = streamBadge(stream.status, t);

  return (
    <div className="layout">
      <Navbar>
        <span className="layout__brand">{t("common.appName")}</span>
        <Badge variant={badge.variant}>{badge.label}</Badge>
      </Navbar>

      <div className="layout__body">
        <Sidebar>
          <nav className="layout__nav" aria-label={t("nav.dashboard")}>
            {navItems(t).map((item, index) => {
              const Icon = ICONS[index] ?? Gauge;

              return (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.to === "/"}
                  className={({ isActive }) =>
                    isActive
                      ? "layout__link layout__link--active"
                      : "layout__link"
                  }
                >
                  <Icon size={16} />
                  <span>{item.label}</span>
                </NavLink>
              );
            })}
          </nav>
        </Sidebar>

        <main className="layout__main">{children}</main>
      </div>

      <Footer copyright={t("common.appName")} />
    </div>
  );
}
