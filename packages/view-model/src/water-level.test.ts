import type { WaterLevelMetricsDto } from "@aquarium/api-client";
import { describe, expect, it } from "vitest";
import { waterLevelCard } from "./water-level";
import { translator } from "@aquarium/i18n";

const t = translator("en");

const healthy: WaterLevelMetricsDto = {
  sump_mechanical: { wet: true, faulty: false },
  sump_biological: { wet: true, faulty: false },
  sump_pump: { wet: true, faulty: false },
  main_tank: { wet: true, faulty: false },
  sump_reserve: { wet: false, faulty: false },
  sump_return_level: { wet: true, faulty: false },
  reservoir_ready: { wet: false, faulty: false },
  reservoir_full: { wet: false, faulty: false },
  sensors_healthy: true,
};

describe("waterLevelCard", () => {
  it("returns the eight sensors in a stable order", () => {
    expect(waterLevelCard(healthy, t).chips.map((c) => c.label)).toEqual([
      "main tank",
      "mechanical",
      "biological",
      "pump",
      "return chamber",
      "reserve",
      "reservoir ready",
      "reservoir full",
    ]);
  });

  it("labels the return chamber and reservoir sensors in polish", () => {
    const labels = waterLevelCard(healthy, translator("pl")).chips.map(
      (c) => c.label,
    );

    expect(labels).toEqual(
      expect.arrayContaining([
        "komora pompy",
        "zapas na podmianę",
        "zbiornik pełny",
      ]),
    );
  });

  it("shows the reservoir and return chamber sensors with their own state", () => {
    const chips = waterLevelCard(
      { ...healthy, reservoir_full: { wet: true, faulty: true } },
      t,
    ).chips;

    expect(chips.find((c) => c.label === "return chamber")?.variant).toBe(
      "wet",
    );
    expect(chips.find((c) => c.label === "reservoir ready")?.variant).toBe(
      "dry",
    );
    expect(chips.find((c) => c.label === "reservoir full")?.variant).toBe(
      "faulty",
    );
  });

  it("maps wet and dry to distinct chip variants", () => {
    const chips = waterLevelCard(healthy, t).chips;

    expect(chips.find((c) => c.label === "main tank")?.variant).toBe("wet");
    expect(chips.find((c) => c.label === "reserve")?.variant).toBe("dry");
  });

  it("marks a faulty sensor regardless of its wet state", () => {
    const card = waterLevelCard(
      {
        ...healthy,
        sump_pump: { wet: true, faulty: true },
      },
      t,
    );

    expect(card.chips.find((c) => c.label === "pump")?.variant).toBe("faulty");
    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("1 sensor faulty");
  });

  it("pluralises the faulty count", () => {
    const card = waterLevelCard(
      {
        ...healthy,
        sump_pump: { wet: true, faulty: true },
        main_tank: { wet: true, faulty: true },
      },
      t,
    );

    expect(card.statusLabel).toBe("2 sensors faulty");
  });

  it("is off when the firmware reports the sensors unhealthy with none faulty", () => {
    const card = waterLevelCard({ ...healthy, sensors_healthy: false }, t);

    expect(card.status).toBe("off");
    expect(card.statusLabel).toBe("sensors off");
  });
});
