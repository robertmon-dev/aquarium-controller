import {
  useLightingSchedule,
  type LightingMetricsDto,
  type Transport,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { formatDuration, useOverrideControl } from "@aquarium/view-model";
import { RotateCcw } from "lucide-react";
import { Alert, Button } from "@/components/atoms";
import { ConfirmDialog } from "@/components/molecules";
import { LightingStatus } from "./LightingStatus";
import { SchedulePreview } from "./SchedulePreview";
import { OverrideControls } from "./OverrideControls";
import { useLocalClock } from "./use-local-clock";
import "./LightingControl.scss";

export const LightingControl = ({
  transport,
  lighting,
}: {
  transport: Transport;
  lighting: LightingMetricsDto;
}) => {
  const t = useT();
  const control = useOverrideControl(transport, lighting);
  const clock = useLocalClock();
  const schedule = useLightingSchedule(transport, clock.month);

  return (
    <>
      {control.active ? (
        <Alert
          variant="warning"
          title={t("lighting.manualActive")}
          action={
            <Button
              variant="warning"
              leftIcon={<RotateCcw size={14} />}
              disabled={control.busy}
              onClick={control.confirmRestore}
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

      <div className="lighting__panels">
        <LightingStatus
          lighting={lighting}
          remaining={control.remaining}
          total={control.total}
          manualChannels={control.manualChannels}
        />

        <OverrideControls
          busy={control.busy}
          duration={control.state.duration}
          levels={control.state.levels}
          selectedProfile={control.selectedProfile}
          onDuration={control.setDuration}
          onProfile={control.pickProfile}
          onChannel={control.setChannel}
          onCommitChannels={control.commitChannels}
        />

        {schedule.data ? (
          <div className="lighting__preview">
            <SchedulePreview
              schedule={schedule.data}
              nowMinute={clock.minuteOfDay}
            />
          </div>
        ) : null}
      </div>

      <ConfirmDialog
        open={control.state.confirming}
        title={t("lighting.restore")}
        message={t("lighting.schedulePaused")}
        confirmText={t("lighting.restore")}
        cancelText={t("common.cancel")}
        variant="primary"
        isLoading={control.busy}
        onCancel={control.cancelRestore}
        onConfirm={control.restore}
      />
    </>
  );
};
