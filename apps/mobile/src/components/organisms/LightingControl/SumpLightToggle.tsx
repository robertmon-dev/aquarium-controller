import {
  useSetSumpLight,
  useSumpLight,
  type SumpLightStatus,
  type Transport,
} from "@aquarium/api-client";
import { screenStyles, spacing } from "@aquarium/theme";
import { formatDuration, sumpLightDurations } from "@aquarium/view-model";
import { useState, type ReactNode } from "react";
import { Text, View } from "react-native";
import { Alert, Button } from "@/components/atoms";
import { DurationPicker, MetricCard } from "@/components/molecules";
import { useT } from "@/i18n/use-locale";

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
      <View style={{ gap: spacing.sm }}>
        {!available && current ? (
          <Alert variant="warning">{t("sumpLight.unavailable")}</Alert>
        ) : null}

        {current?.expires_in_s != null ? (
          <Text style={screenStyles.bodyText}>
            {t("sumpLight.expiresIn")} {formatDuration(current.expires_in_s)}
          </Text>
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
          onPress={() =>
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
      </View>
    </MetricCard>
  );
};
