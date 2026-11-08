import type { LightingProfile } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { LIGHTING_PROFILES } from "@aquarium/view-model";
import { Button } from "@/components/atoms";
import "./ProfilePicker.scss";

export function ProfilePicker({
  selected,
  disabled,
  onSelect,
}: {
  selected: LightingProfile | null;
  disabled?: boolean;
  onSelect: (profile: LightingProfile) => void;
}) {
  const t = useT();

  return (
    <div className="picker">
      {LIGHTING_PROFILES.map((profile) => (
        <Button
          key={profile}
          variant={profile === selected ? "primary" : "outline"}
          disabled={disabled ?? false}
          onClick={() => onSelect(profile)}
        >
          {t(`profiles.${profile}`)}
        </Button>
      ))}
    </div>
  );
}
