import { describe, expect, it } from "vitest";
import {
  EM_DASH,
  formatCelsius,
  formatDuration,
  formatHeap,
  formatLitresPerHour,
  formatMicrosiemens,
  formatPercent,
  formatPh,
  formatPpm,
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
  it("renders kibibytes below a mebibyte", () => {
    expect(formatHeap(512)).toBe("512 KiB");
  });

  it("switches to mebibytes at 1024, matching the firmware bytes/1024", () => {
    expect(formatHeap(8302)).toBe("8.1 MiB");
  });
});

describe("formatRssi", () => {
  it("renders dBm", () => {
    expect(formatRssi(-54)).toBe("-54 dBm");
  });

  it("renders an unreadable signal as a dash rather than 0 dBm", () => {
    expect(formatRssi(null)).toBe(EM_DASH);
  });
});

describe("formatPpm", () => {
  it("rounds to whole parts per million", () => {
    expect(formatPpm(219.6)).toBe("220 ppm");
  });

  it("renders a dash when there is no reading", () => {
    expect(formatPpm(null)).toBe(EM_DASH);
  });

  it("keeps a dry probe at zero rather than showing a dash", () => {
    expect(formatPpm(0)).toBe("0 ppm");
  });
});

describe("formatMicrosiemens", () => {
  it("rounds to whole microsiemens per centimetre", () => {
    expect(formatMicrosiemens(439.85)).toBe("440 \u00b5S/cm");
  });

  it("renders a dash when there is no reading", () => {
    expect(formatMicrosiemens(null)).toBe(EM_DASH);
  });
});
