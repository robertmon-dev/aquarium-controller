import {
  useClearLightingOverride,
  useLightingOverride,
  useSetLightingOverride,
  type ChannelLevels,
  type LightingMetricsDto,
  type LightingProfile,
  type Transport,
} from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { useState } from "react";
import { Text, View } from "react-native";
import { Alert, Button } from "@/components/atoms";
import {
  ChannelSliders,
  DurationPicker,
  MetricCard,
  ProfilePicker,
} from "@/components/molecules";
import { formatDuration, formatPercentPair } from "@/components/utility/format";
import {
  channelRequest,
  profileRequest,
} from "@/components/utility/lighting-request";
import { useCountdown } from "@/components/utility/use-countdown";
import { useT } from "@/i18n/use-locale";

export function LightingControl({
  transport,
  lighting,
}: {
  transport: Transport;
  lighting: LightingMetricsDto;
}) {
  const t = useT();
  const override = useLightingOverride(transport);
  const setOverride = useSetLightingOverride(transport);
  const clearOverride = useClearLightingOverride(transport);

  const [duration, setDuration] = useState<number | null>(900);
  const [levels, setLevels] = useState<ChannelLevels>({
    white_pct: lighting.channel_white_pct,
    rgb_pct: lighting.channel_rgb_pct,
  });

  const remaining = useCountdown(override.data?.expires_in_s ?? null, () => {
    void override.refetch();
  });

  const busy = setOverride.isPending || clearOverride.isPending;
  const active = lighting.override_active;

  const selectedProfile: LightingProfile | null =
    override.data && "profile" in override.data.target
      ? override.data.target.profile
      : null;

  const failure = setOverride.error ?? clearOverride.error;

  return (
    <View style={{ gap: spacing.sm }}>
      <MetricCard
        title={t("lighting.now")}
        status={active ? "degraded" : "ok"}
        statusLabel={active ? t("lighting.override") : t("lighting.onSchedule")}
        metrics={[
          {
            label: t("lighting.profile"),
            value: t(`profiles.${lighting.profile}`),
          },
          {
            label: t("lighting.whiteRgb"),
            value: formatPercentPair(
              lighting.channel_white_pct,
              lighting.channel_rgb_pct,
            ),
          },
          ...(remaining === null
            ? []
            : [
                {
                  label: t("lighting.expiresIn"),
                  value: formatDuration(remaining),
                },
              ]),
        ]}
      />

      {failure ? (
        <Alert variant="danger" title={t("lighting.refused")}>
          {failure.message}
        </Alert>
      ) : null}

      <MetricCard
        title={t("lighting.override")}
        status="ok"
        statusLabel={busy ? t("lighting.sending") : t("lighting.ready")}
      >
        <View style={{ gap: spacing.sm }}>
          <DurationPicker selected={duration} onSelect={setDuration} />

          <ProfilePicker
            selected={selectedProfile}
            disabled={busy}
            onSelect={(profile) =>
              setOverride.mutate(profileRequest(profile, duration))
            }
          />

          <ChannelSliders
            levels={levels}
            disabled={busy}
            onCommit={(next) => {
              setLevels(next);
              setOverride.mutate(channelRequest(next, duration));
            }}
          />

          <Text style={cardStyles.sectionLabel}>
            {active
              ? t("lighting.schedulePaused")
              : t("lighting.scheduleRunning")}
          </Text>

          <Button
            variant="outline"
            fullWidth
            isLoading={clearOverride.isPending}
            disabled={!active}
            onPress={() => clearOverride.mutate()}
          >
            {t("lighting.restore")}
          </Button>
        </View>
      </MetricCard>
    </View>
  );
}
