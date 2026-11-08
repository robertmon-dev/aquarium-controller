import {
  useClearLightingOverride,
  useLightingOverride,
  useSetLightingOverride,
  type LightingMetricsDto,
  type LightingProfile,
  type Transport,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import {
  channelRequest,
  formatDuration,
  formatPercentPair,
  profileRequest,
  useCountdown,
} from "@aquarium/view-model";
import { useReducer, useState } from "react";
import { Alert, Button } from "@/components/atoms";
import { ConfirmDialog, MetricCard } from "@/components/molecules";
import { ChannelSliders } from "@/components/molecules/ChannelSliders/ChannelSliders";
import { DurationPicker } from "@/components/molecules/DurationPicker/DurationPicker";
import { ProfilePicker } from "@/components/molecules/ProfilePicker/ProfilePicker";
import { initialLightingState, lightingReducer } from "./lighting-reducer";

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
  const [confirming, setConfirming] = useState(false);

  const [state, dispatch] = useReducer(
    lightingReducer,
    {
      white_pct: lighting.channel_white_pct,
      rgb_pct: lighting.channel_rgb_pct,
    },
    initialLightingState,
  );

  const remaining = useCountdown(override.data?.expires_in_s ?? null, () => {
    void override.refetch();
  });

  const busy = setOverride.isPending || clearOverride.isPending;
  const active = lighting.override_active;
  const failure = setOverride.error ?? clearOverride.error;

  const selectedProfile: LightingProfile | null =
    state.mode === "profile" && state.profile
      ? state.profile
      : override.data && "profile" in override.data.target
        ? override.data.target.profile
        : null;

  return (
    <>
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
        <DurationPicker
          selected={state.duration}
          onSelect={(seconds) => dispatch({ type: "setDuration", seconds })}
        />

        <ProfilePicker
          selected={selectedProfile}
          disabled={busy}
          onSelect={(profile) => {
            dispatch({ type: "pickProfile", profile });
            setOverride.mutate(profileRequest(profile, state.duration), {
              onSuccess: () => dispatch({ type: "committed" }),
            });
          }}
        />

        <ChannelSliders
          levels={state.levels}
          disabled={busy}
          onChange={(channel, value) =>
            dispatch({ type: "setChannel", channel, value })
          }
          onCommit={() => {
            if (!state.dirty) return;
            setOverride.mutate(channelRequest(state.levels, state.duration), {
              onSuccess: () => dispatch({ type: "committed" }),
            });
          }}
        />

        <p className="lighting__note">
          {active ? t("lighting.schedulePaused") : t("lighting.scheduleRunning")}
        </p>

        <Button
          variant="outline"
          fullWidth
          disabled={!active || busy}
          onClick={() => setConfirming(true)}
        >
          {t("lighting.restore")}
        </Button>
      </MetricCard>

      {confirming ? (
        <ConfirmDialog
          message={t("lighting.restore")}
          confirmText={t("lighting.restore")}
          cancelText={t("common.cancel")}
          variant="primary"
          isLoading={clearOverride.isPending}
          onCancel={() => setConfirming(false)}
          onConfirm={() =>
            clearOverride.mutate(undefined, {
              onSuccess: () => {
                dispatch({
                  type: "restore",
                  levels: {
                    white_pct: lighting.channel_white_pct,
                    rgb_pct: lighting.channel_rgb_pct,
                  },
                });
                setConfirming(false);
              },
            })
          }
        />
      ) : null}
    </>
  );
}
