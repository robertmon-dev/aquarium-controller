import { describe, expect, it } from "vitest";
import {
  calibrationFormSchema,
  toCalibrationFormValues,
  toCalibrationPatch,
  type CalibrationFormMessages,
  type CalibrationFormValues,
} from "./calibration-form";
import type { Calibration } from "./generated";

const messages: CalibrationFormMessages = {
  numberRequired: "number required",
  spanTooSmall: "span too small",
  makeupRange: "makeup out of range",
  factorRange: "factor out of range",
};

const schema = calibrationFormSchema(messages);

const current: Calibration = {
  ph: {
    neutral_volts: 1.5,
    acid_volts: 1.67748,
    calibrated_at_celsius: 25,
  },
  makeup_tank_celsius: 25,
  tds_factor: 1,
};

const values = (overrides: Partial<CalibrationFormValues> = {}) => ({
  neutralVolts: "1.5",
  acidVolts: "1.67748",
  calibratedAtCelsius: "25",
  makeupTankCelsius: "25",
  tdsFactor: "1",
  ...overrides,
});

describe("calibrationFormSchema", () => {
  it("accepts a sane calibration", () => {
    expect(schema.safeParse(values()).success).toBe(true);
  });

  it("accepts a comma as the decimal separator, because phone keyboards offer it", () => {
    const parsed = schema.safeParse(values({ makeupTankCelsius: "22,5" }));

    expect(parsed.success).toBe(true);
    if (parsed.success) expect(parsed.data.makeupTankCelsius).toBeCloseTo(22.5);
  });

  it("rejects an empty field", () => {
    expect(schema.safeParse(values({ tdsFactor: "" })).success).toBe(false);
  });

  it("rejects text where a number belongs", () => {
    expect(schema.safeParse(values({ neutralVolts: "abc" })).success).toBe(
      false,
    );
  });

  it("rejects two buffer points closer than 30 mV, mirroring the device", () => {
    const parsed = schema.safeParse(
      values({ neutralVolts: "1.500", acidVolts: "1.510" }),
    );

    expect(parsed.success).toBe(false);
    if (!parsed.success) {
      expect(parsed.error.issues[0]?.message).toBe(messages.spanTooSmall);
    }
  });

  it("accepts an inverted board, where the acid point sits below neutral", () => {
    expect(
      schema.safeParse(values({ neutralVolts: "1.5", acidVolts: "1.32252" }))
        .success,
    ).toBe(true);
  });

  it("rejects a makeup temperature outside the band the device accepts", () => {
    expect(schema.safeParse(values({ makeupTankCelsius: "-1" })).success).toBe(
      false,
    );
    expect(schema.safeParse(values({ makeupTankCelsius: "41" })).success).toBe(
      false,
    );
  });

  it("rejects a TDS factor outside the band the device accepts", () => {
    expect(schema.safeParse(values({ tdsFactor: "0.05" })).success).toBe(false);
    expect(schema.safeParse(values({ tdsFactor: "11" })).success).toBe(false);
  });
});

describe("toCalibrationFormValues", () => {
  it("fills every field from the stored calibration", () => {
    expect(toCalibrationFormValues(current)).toEqual({
      neutralVolts: "1.5",
      acidVolts: "1.67748",
      calibratedAtCelsius: "25",
      makeupTankCelsius: "25",
      tdsFactor: "1",
    });
  });
});

describe("toCalibrationPatch", () => {
  const parse = (overrides: Partial<CalibrationFormValues> = {}) => {
    const parsed = schema.safeParse(values(overrides));
    if (!parsed.success) throw new Error("fixture does not validate");
    return parsed.data;
  };

  it("sends nothing when nothing changed", () => {
    expect(toCalibrationPatch(parse(), current)).toEqual({
      ph: null,
      makeup_tank_celsius: null,
      tds_factor: null,
    });
  });

  it("sends only the field that changed", () => {
    const patch = toCalibrationPatch(
      parse({ makeupTankCelsius: "22.5" }),
      current,
    );

    expect(patch.makeup_tank_celsius).toBeCloseTo(22.5);
    expect(patch.ph).toBeNull();
    expect(patch.tds_factor).toBeNull();
  });

  it("sends all three pH points when any one of them moves, because the device validates them together", () => {
    const patch = toCalibrationPatch(parse({ acidVolts: "1.66" }), current);

    expect(patch.ph).not.toBeNull();
    expect(patch.ph?.neutral_volts).toBeCloseTo(1.5);
    expect(patch.ph?.acid_volts).toBeCloseTo(1.66);
    expect(patch.ph?.calibrated_at_celsius).toBeCloseTo(25);
  });

  it("treats a rewritten but identical value as unchanged", () => {
    const patch = toCalibrationPatch(parse({ tdsFactor: "1.0" }), current);

    expect(patch.tds_factor).toBeNull();
  });
});
