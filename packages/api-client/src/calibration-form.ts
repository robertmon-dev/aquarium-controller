import { z } from "zod";
import type { Calibration, CalibrationPatch } from "./generated";

export const MIN_BUFFER_SPAN_VOLTS = 0.03;
export const MIN_MAKEUP_CELSIUS = 0;
export const MAX_MAKEUP_CELSIUS = 40;
export const MIN_TDS_FACTOR = 0.1;
export const MAX_TDS_FACTOR = 10;

export interface CalibrationFormMessages {
  numberRequired: string;
  spanTooSmall: string;
  makeupRange: string;
  factorRange: string;
}

const decimal = (message: string) =>
  z
    .string()
    .trim()
    .refine((value) => value !== "", message)
    .transform((value) => Number(value.replace(",", ".")))
    .refine((value) => Number.isFinite(value), message);

export const calibrationFormSchema = (m: CalibrationFormMessages) => {
  return z
    .object({
      neutralVolts: decimal(m.numberRequired),
      acidVolts: decimal(m.numberRequired),
      calibratedAtCelsius: decimal(m.numberRequired),
      makeupTankCelsius: decimal(m.numberRequired).refine(
        (value) => value >= MIN_MAKEUP_CELSIUS && value <= MAX_MAKEUP_CELSIUS,
        m.makeupRange,
      ),
      tdsFactor: decimal(m.numberRequired).refine(
        (value) => value >= MIN_TDS_FACTOR && value <= MAX_TDS_FACTOR,
        m.factorRange,
      ),
    })
    .refine(
      (values) =>
        Math.abs(values.acidVolts - values.neutralVolts) >=
        MIN_BUFFER_SPAN_VOLTS,
      { message: m.spanTooSmall, path: ["acidVolts"] },
    );
};

export interface CalibrationFormValues {
  neutralVolts: string;
  acidVolts: string;
  calibratedAtCelsius: string;
  makeupTankCelsius: string;
  tdsFactor: string;
}

export type ParsedCalibrationForm = z.output<
  ReturnType<typeof calibrationFormSchema>
>;

export const toCalibrationFormValues = (
  calibration: Calibration,
): CalibrationFormValues => {
  return {
    neutralVolts: String(calibration.ph.neutral_volts),
    acidVolts: String(calibration.ph.acid_volts),
    calibratedAtCelsius: String(calibration.ph.calibrated_at_celsius),
    makeupTankCelsius: String(calibration.makeup_tank_celsius),
    tdsFactor: String(calibration.tds_factor),
  };
};

const changed = (next: number, current: number): boolean => {
  return Math.abs(next - current) > 1e-6;
};

export const toCalibrationPatch = (
  parsed: ParsedCalibrationForm,
  current: Calibration,
): CalibrationPatch => {
  const phChanged =
    changed(parsed.neutralVolts, current.ph.neutral_volts) ||
    changed(parsed.acidVolts, current.ph.acid_volts) ||
    changed(parsed.calibratedAtCelsius, current.ph.calibrated_at_celsius);

  return {
    ph: phChanged
      ? {
          neutral_volts: parsed.neutralVolts,
          acid_volts: parsed.acidVolts,
          calibrated_at_celsius: parsed.calibratedAtCelsius,
        }
      : null,
    makeup_tank_celsius: changed(
      parsed.makeupTankCelsius,
      current.makeup_tank_celsius,
    )
      ? parsed.makeupTankCelsius
      : null,
    tds_factor: changed(parsed.tdsFactor, current.tds_factor)
      ? parsed.tdsFactor
      : null,
  };
};
