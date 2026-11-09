import type { ChannelLevels, LightingProfile } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { MetricCard } from "@/components/molecules";
import { ChannelSliders } from "@/components/molecules/ChannelSliders/ChannelSliders";
import { DurationPicker } from "@/components/molecules/DurationPicker/DurationPicker";
import { ProfilePicker } from "@/components/molecules/ProfilePicker/ProfilePicker";
import "./LightingControl.scss";

export const OverrideControls = ({
  busy,
  duration,
  levels,
  selectedProfile,
  onDuration,
  onProfile,
  onChannel,
  onCommitChannels,
}: {
  busy: boolean;
  duration: number | null;
  levels: ChannelLevels;
  selectedProfile: LightingProfile | null;
  onDuration: (seconds: number | null) => void;
  onProfile: (profile: LightingProfile) => void;
  onChannel: (channel: keyof ChannelLevels, value: number) => void;
  onCommitChannels: () => void;
}) => {
  const t = useT();

  return (
    <MetricCard
      title={t("lighting.override")}
      status="ok"
      statusLabel={busy ? t("lighting.sending") : t("lighting.ready")}
    >
      <div className="override">
        <section className="override__group">
          <h4 className="override__label">{t("lighting.duration")}</h4>
          <DurationPicker selected={duration} onSelect={onDuration} />
        </section>

        <section className="override__group">
          <h4 className="override__label">{t("lighting.profile")}</h4>
          <ProfilePicker
            selected={selectedProfile}
            disabled={busy}
            onSelect={onProfile}
          />
        </section>

        <section className="override__group">
          <h4 className="override__label">{t("lighting.manualChannels")}</h4>
          <ChannelSliders
            levels={levels}
            disabled={busy}
            onChange={onChannel}
            onCommit={onCommitChannels}
          />
        </section>
      </div>
    </MetricCard>
  );
};
