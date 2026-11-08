import type { LightingMetricsDto, Transport } from "@aquarium/api-client";
import { spacing } from "@aquarium/theme";
import {
  formatDuration,
  formatPercentPair,
  useOverrideControl,
} from "@aquarium/view-model";
import { useEffect, useRef } from "react";
import { Alert as RNAlert, View } from "react-native";
import { Alert, Button, Meter } from "@/components/atoms";
import {
  ChannelSliders,
  DurationPicker,
  MetricCard,
  ProfilePicker,
} from "@/components/molecules";
import { useT } from "@/i18n/use-locale";

export function LightingControl({
  transport,
  lighting,
}: {
  transport: Transport;
  lighting: LightingMetricsDto;
}) {
  const t = useT();
  const control = useOverrideControl(transport, lighting);
  const { confirming } = control.state;

  // The handlers are deliberately not memoised, so listing them as effect
  // dependencies would reopen the dialog on every render. A ref keeps the
  // dependency list honest and still calls the current handler — written in an
  // effect, because writing to a ref during render breaks the rules of React.
  const latest = useRef(control);

  useEffect(() => {
    latest.current = control;
  });

  // The shared reducer carries `confirming`; on mobile the platform's own
  // dialog answers it. Without this effect the restore button would set a flag
  // and nothing else would ever happen.
  useEffect(() => {
    if (!confirming) return;

    RNAlert.alert(t("lighting.restore"), t("lighting.schedulePaused"), [
      {
        text: t("common.cancel"),
        style: "cancel",
        onPress: () => latest.current.cancelRestore(),
      },
      { text: t("lighting.restore"), onPress: () => latest.current.restore() },
    ]);
  }, [confirming, t]);

  return (
    <View style={{ gap: spacing.sm }}>
      {control.active ? (
        <Alert
          variant="warning"
          title={t("lighting.manualActive")}
          action={
            <Button
              variant="warning"
              disabled={control.busy}
              onPress={control.confirmRestore}
            >
              {t("lighting.restore")}
            </Button>
          }
        >
          {control.remaining === null
            ? t("lighting.schedulePaused")
            : t("lighting.manualBody", {
                remaining: formatDuration(control.remaining),
              })}
        </Alert>
      ) : null}

      {control.failure ? (
        <Alert variant="danger" title={t("lighting.refused")}>
          {control.failure.message}
        </Alert>
      ) : null}

      <MetricCard
        title={t("lighting.currentState")}
        status={control.active ? "degraded" : "ok"}
        statusLabel={
          control.active ? t("lighting.override") : t("lighting.onSchedule")
        }
        metrics={[
          {
            label: t("lighting.profile"),
            value: control.manualChannels
              ? t("lighting.custom")
              : t(`profiles.${lighting.profile}`),
          },
          {
            label: t("lighting.whiteRgb"),
            value: formatPercentPair(
              lighting.channel_white_pct,
              lighting.channel_rgb_pct,
            ),
          },
        ]}
      >
        {control.remaining !== null &&
        control.total !== null &&
        control.total > 0 ? (
          <Meter
            value={control.remaining / control.total}
            label={t("lighting.expiresIn")}
            valueText={formatDuration(control.remaining)}
          />
        ) : null}
      </MetricCard>

      <MetricCard
        title={t("lighting.override")}
        status="ok"
        statusLabel={control.busy ? t("lighting.sending") : t("lighting.ready")}
      >
        <View style={{ gap: spacing.sm }}>
          <DurationPicker
            selected={control.state.duration}
            onSelect={control.setDuration}
          />

          <ProfilePicker
            selected={control.selectedProfile}
            disabled={control.busy}
            onSelect={control.pickProfile}
          />

          <ChannelSliders
            levels={control.state.levels}
            disabled={control.busy}
            onCommit={control.commitLevels}
          />
        </View>
      </MetricCard>
    </View>
  );
}
