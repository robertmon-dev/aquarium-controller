import { describe, expect, it } from "vitest";
import {
  EM_DASH,
  formatCelsius,
  formatDuration,
  formatHeap,
  formatLitresPerHour,
  formatPercent,
  formatPercentPair,
  formatPh,
  formatRssi,
} from "./format";

describe("formatCelsius", () => {
  it("renders two decimals with a unit", () => {
    expect(formatCelsius(25.5)).toBe("25.50 °C");
  });

  it("renders null as an em dash, never as zero", () => {
    expect(formatCelsius(null)).toBe(EM_DASH);
  });
});

describe("formatPh", () => {
  it("renders two decimals", () => {
    expect(formatPh(6.8)).toBe("6.80");
  });

  it("renders null as an em dash", () => {
    expect(formatPh(null)).toBe(EM_DASH);
  });
});

describe("formatLitresPerHour", () => {
  it("renders a rounded rate with a unit", () => {
    expect(formatLitresPerHour(640.4)).toBe("640 L/h");
  });

  it("renders null as an em dash", () => {
    expect(formatLitresPerHour(null)).toBe(EM_DASH);
  });
});

describe("formatPercent", () => {
  it("appends a percent sign", () => {
    expect(formatPercent(60)).toBe("60%");
  });

  it("renders null as an em dash", () => {
    expect(formatPercent(null)).toBe(EM_DASH);
  });
});

describe("formatPercentPair", () => {
  it("joins two channels", () => {
    expect(formatPercentPair(60, 40)).toBe("60% / 40%");
  });
});

describe("formatDuration", () => {
  it("renders seconds under a minute", () => {
    expect(formatDuration(42)).toBe("42s");
  });

  it("renders minutes and seconds", () => {
    expect(formatDuration(125)).toBe("2m 05s");
  });

  it("renders hours and minutes", () => {
    expect(formatDuration(3700)).toBe("1h 01m");
  });

  it("renders zero", () => {
    expect(formatDuration(0)).toBe("0s");
  });
});

describe("formatHeap", () => {
  it("renders kilobytes below a megabyte", () => {
    expect(formatHeap(512)).toBe("512 kB");
  });

  it("switches to megabytes above a thousand", () => {
    expect(formatHeap(8302)).toBe("8.3 MB");
  });
});

describe("formatRssi", () => {
  it("renders dBm", () => {
    expect(formatRssi(-54)).toBe("-54 dBm");
  });
});
