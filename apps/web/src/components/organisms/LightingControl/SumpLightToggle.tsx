import {
  useSetSumpLight,
  useSumpLight,
  type SumpLightStatus,
  type Transport,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { formatDuration, sumpLightDurations } from "@aquarium/view-model";
import { useState, type ReactNode } from "react";
import { Alert, Button } from "@/components/atoms";
import { MetricCard } from "@/components/molecules";
import { DurationPicker } from "@/components/molecules/DurationPicker/DurationPicker";
import "./LightingControl.scss";

export const SumpLightToggle = ({
  transport,
  status,
}: {
  transport: Transport;
  status: SumpLightStatus | undefined;
}): ReactNode => {
  const t = useT();
  const fallback = useSumpLight(transport);
  const setSumpLight = useSetSumpLight(transport);
  const [duration, setDuration] = useState<number | null>(1800);

  const current = status ?? fallback.data;
  const on = current?.on ?? false;
  const available = current?.available ?? false;

  return (
    <MetricCard
      title={t("sumpLight.title")}
      status={available ? "ok" : "degraded"}
      statusLabel={on ? t("sumpLight.on") : t("sumpLight.off")}
    >
      <div className="override">
        {!available && current ? (
          <Alert variant="warning">{t("sumpLight.unavailable")}</Alert>
        ) : null}

        {current?.expires_in_s != null ? (
          <p>
            {t("sumpLight.expiresIn")} {formatDuration(current.expires_in_s)}
          </p>
        ) : null}

        {on ? null : (
          <DurationPicker
            selected={duration}
            onSelect={setDuration}
            choices={sumpLightDurations(t)}
          />
        )}

        <Button
          variant={on ? "outline" : "primary"}
          disabled={setSumpLight.isPending}
          onClick={() =>
            setSumpLight.mutate(
              on
                ? { on: false, duration_s: null }
                : { on: true, duration_s: duration },
            )
          }
        >
          {on ? t("sumpLight.turnOff") : t("sumpLight.turnOn")}
        </Button>

        {setSumpLight.error ? (
          <Alert variant="danger" title={t("lighting.refused")}>
            {setSumpLight.error.message}
          </Alert>
        ) : null}
      </div>
    </MetricCard>
  );
};
