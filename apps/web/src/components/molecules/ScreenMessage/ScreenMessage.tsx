import type { ReactNode } from "react";
import { Spinner } from "@/components/atoms";
import "./ScreenMessage.scss";

export function ScreenMessage({
  children,
  loading = false,
}: {
  children: ReactNode;
  loading?: boolean;
}) {
  return (
    <div className="screen-message">
      {loading ? <Spinner size="lg" /> : null}
      <p className="screen-message__text">{children}</p>
    </div>
  );
}
