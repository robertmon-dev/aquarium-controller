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

export function LightingControl({
  transport,
  lighting,
}: {
  transport: Transport;
  lighting: LightingMetricsDto;
}) {
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
        title="now"
        status={active ? "degraded" : "ok"}
        statusLabel={active ? "override" : "on schedule"}
        metrics={[
          { label: "profile", value: lighting.profile },
          {
            label: "white / rgb",
            value: formatPercentPair(
              lighting.channel_white_pct,
              lighting.channel_rgb_pct,
            ),
          },
          ...(remaining === null
            ? []
            : [{ label: "expires in", value: formatDuration(remaining) }]),
        ]}
      />

      {failure ? (
        <Alert variant="danger" title="the device refused that">
          {failure.message}
        </Alert>
      ) : null}

      <MetricCard
        title="override"
        status="ok"
        statusLabel={busy ? "sending…" : "ready"}
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
              ? "an override is active — the schedule is paused"
              : "the schedule is running"}
          </Text>

          <Button
            variant="outline"
            fullWidth
            isLoading={clearOverride.isPending}
            disabled={!active}
            onPress={() => clearOverride.mutate()}
          >
            restore schedule
          </Button>
        </View>
      </MetricCard>
    </View>
  );
}
