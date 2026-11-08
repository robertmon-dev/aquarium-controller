import type { Transport } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { Settings } from "lucide-react";
import type { ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/atoms";
import { ErrorPage } from "@/pages/Error/Error";
import { useDevice } from "@/providers/AppProviders";

export function DeviceGate({
  children,
}: {
  children: (transport: Transport) => ReactNode;
}) {
  const t = useT();
  const navigate = useNavigate();
  const { transport, loading } = useDevice();

  if (loading) return null;

  if (!transport) {
    return (
      <ErrorPage
        code="not-configured"
        action={
          <Button variant="primary" onClick={() => navigate("/settings")}>
            <Settings size={18} />
            {t("errors.openSettings")}
          </Button>
        }
      />
    );
  }

  return <>{children(transport)}</>;
}
