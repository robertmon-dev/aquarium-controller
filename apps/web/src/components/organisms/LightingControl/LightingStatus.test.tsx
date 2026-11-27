import type { LightingMetricsDto } from "@aquarium/api-client";
import { memoryAdapter } from "@aquarium/i18n/adapter";
import { LocaleProvider } from "@aquarium/i18n/use-locale";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LightingStatus } from "./LightingStatus";

const lighting: LightingMetricsDto = {
  profile: "daylight",
  channels: { red_pct: 10, green_pct: 20, blue_pct: 30, white_pct: 40 },
  override_active: false,
  available: true,
};

function renderStatus(value: LightingMetricsDto) {
  render(
    <LocaleProvider adapter={memoryAdapter({ stored: "en", system: "en" })}>
      <LightingStatus
        lighting={value}
        remaining={null}
        total={null}
        manualChannels={false}
      />
    </LocaleProvider>,
  );
}

describe("LightingStatus", () => {
  it("shows a meter per channel while the lighting loop is running", () => {
    renderStatus(lighting);

    expect(screen.getAllByRole("meter")).toHaveLength(4);
    expect(screen.queryByText("lighting control is not running")).toBeNull();
  });

  it("warns instead of showing zero channels while the lighting loop is not running", () => {
    renderStatus({ ...lighting, available: false });

    expect(screen.getByText("lighting control is not running")).toBeTruthy();
    expect(screen.queryAllByRole("meter")).toHaveLength(0);
  });
});
