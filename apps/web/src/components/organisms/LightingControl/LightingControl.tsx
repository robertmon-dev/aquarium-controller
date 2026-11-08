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
  profileRequest,
  useCountdown,
} from "@aquarium/view-model";
import { RotateCcw } from "lucide-react";
import { useReducer, useRef } from "react";
import { Alert, Button } from "@/components/atoms";
import { ConfirmDialog } from "@/components/molecules";
import { LightingStatus } from "./LightingStatus";
import { OverrideControls } from "./OverrideControls";
import { initialLightingState, lightingReducer } from "./lighting-reducer";
import "./LightingControl.scss";

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

  const total = useRef<number | null>(null);
  if (remaining === null) total.current = null;
  else if (total.current === null || remaining > total.current)
    total.current = remaining;

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
      {active ? (
        <Alert
          variant="warning"
          title={t("lighting.manualActive")}
          action={
            <Button
              variant="warning"
              leftIcon={<RotateCcw size={14} />}
              disabled={busy}
              onClick={() => dispatch({ type: "confirmRestore" })}
            >
              {t("lighting.restore")}
            </Button>
          }
        >
          {remaining === null
            ? t("lighting.schedulePaused")
            : t("lighting.manualBody", {
                remaining: formatDuration(remaining),
              })}
        </Alert>
      ) : null}

      {failure ? (
        <Alert variant="danger" title={t("lighting.refused")}>
          {failure.message}
        </Alert>
      ) : null}

      <div className="lighting__panels">
        <LightingStatus
          lighting={lighting}
          remaining={remaining}
          total={total.current}
        />

        <OverrideControls
          busy={busy}
          duration={state.duration}
          levels={state.levels}
          selectedProfile={selectedProfile}
          onDuration={(seconds) => dispatch({ type: "setDuration", seconds })}
          onProfile={(profile) => {
            dispatch({ type: "pickProfile", profile });
            setOverride.mutate(profileRequest(profile, state.duration), {
              onSuccess: () => dispatch({ type: "committed" }),
            });
          }}
          onChannel={(channel, value) =>
            dispatch({ type: "setChannel", channel, value })
          }
          onCommitChannels={() => {
            if (!state.dirty) return;
            setOverride.mutate(channelRequest(state.levels, state.duration), {
              onSuccess: () => dispatch({ type: "committed" }),
            });
          }}
        />
      </div>

      <ConfirmDialog
        open={state.confirming}
        title={t("lighting.restore")}
        message={t("lighting.schedulePaused")}
        confirmText={t("lighting.restore")}
        cancelText={t("common.cancel")}
        variant="primary"
        isLoading={clearOverride.isPending}
        onCancel={() => dispatch({ type: "cancelRestore" })}
        onConfirm={() =>
          clearOverride.mutate(undefined, {
            onSuccess: () =>
              dispatch({
                type: "restore",
                levels: {
                  white_pct: lighting.channel_white_pct,
                  rgb_pct: lighting.channel_rgb_pct,
                },
              }),
          })
        }
      />
    </>
  );
}
