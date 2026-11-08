import { useMemo } from "react";
import { useT } from "@aquarium/i18n/use-locale";
import { ShieldAlert, ShieldCheck, Info, AlertTriangle, X } from "lucide-react";
import type { AlertProps } from "./types";
import "./Alert.scss";

export const Alert = ({
  variant = "info",
  title,
  children,
  onClose,
  className = "",
}: AlertProps) => {
  const t = useT();

  const Icon = useMemo(() => {
    switch (variant) {
      case "success":
        return ShieldCheck;
      case "warning":
        return AlertTriangle;
      case "danger":
        return ShieldAlert;
      default:
        return Info;
    }
  }, [variant]);

  return (
    <div className={`alert alert--${variant} ${className}`} role="alert">
      <div className="alert__icon">
        <Icon size={20} />
      </div>

      <div className="alert__body">
        {title && <strong className="alert__title">{title}</strong>}
        <div className="alert__description">{children}</div>
      </div>

      {onClose && (
        <button
          className="alert__close"
          onClick={onClose}
          aria-label={t("common.close")}
          type="button"
        >
          <X size={18} strokeWidth={2.5} />
        </button>
      )}
    </div>
  );
};
