import { useT } from "@aquarium/i18n/use-locale";
import { ArrowLeft, Home } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/atoms";
import { ERROR_CONFIG } from "./consts";
import type { ErrorPageProps } from "./types";
import "./Error.scss";

export function ErrorPage({
  code,
  title,
  message,
  action,
  className = "",
}: ErrorPageProps) {
  const t = useT();
  const navigate = useNavigate();
  const config = ERROR_CONFIG[code];
  const Icon = config.icon;

  return (
    <div className={`error-page error-page--${code} ${className}`.trim()}>
      <div className="error-page__container">
        <div className="error-page__header">
          <div className="error-page__icon-wrapper">
            <Icon size={96} strokeWidth={1.5} />
          </div>
          <h1 className="error-page__code">{config.badge}</h1>
        </div>

        <div className="error-page__content">
          <div className="error-page__text">
            <h2 className="error-page__title">{title ?? t(config.titleKey)}</h2>
            <p className="error-page__description">
              {message ?? t(config.descKey)}
            </p>
          </div>
        </div>

        <div className="error-page__actions">
          {action ?? (
            <>
              <Button variant="ghost" onClick={() => navigate(-1)}>
                <ArrowLeft size={18} />
                {t("errors.goBack")}
              </Button>

              <Button variant="primary" onClick={() => navigate("/")}>
                <Home size={18} />
                {t("nav.dashboard")}
              </Button>
            </>
          )}
        </div>
      </div>

      <div className="error-page__glow" aria-hidden="true" />
    </div>
  );
}
