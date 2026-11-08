import type { ChannelLevels } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { formatPercent } from "@aquarium/view-model";
import "./ChannelSliders.scss";

export function ChannelSliders({
  levels,
  disabled,
  onChange,
  onCommit,
}: {
  levels: ChannelLevels;
  disabled?: boolean;
  onChange: (channel: keyof ChannelLevels, value: number) => void;
  onCommit: () => void;
}) {
  const t = useT();

  const rows: { channel: keyof ChannelLevels; label: string }[] = [
    { channel: "white_pct", label: t("lighting.white") },
    { channel: "rgb_pct", label: t("lighting.rgb") },
  ];

  return (
    <div className="sliders">
      {rows.map((row) => (
        <label className="sliders__row" key={row.channel}>
          <span className="sliders__label">{row.label}</span>
          <input
            className="sliders__input"
            type="range"
            min={0}
            max={100}
            step={1}
            value={levels[row.channel]}
            disabled={disabled ?? false}
            onChange={(event) =>
              onChange(row.channel, Number(event.target.value))
            }
            onMouseUp={onCommit}
            onTouchEnd={onCommit}
            onKeyUp={onCommit}
          />
          <span className="sliders__value">
            {formatPercent(levels[row.channel])}
          </span>
        </label>
      ))}
    </div>
  );
}
