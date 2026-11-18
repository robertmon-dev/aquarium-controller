import type { CalibrationFormValues } from "@aquarium/api-client";
import type { ProbeStability } from "@aquarium/view-model";
import type { FieldErrors, UseFormRegister } from "react-hook-form";

export type CalibrationRegister = UseFormRegister<CalibrationFormValues>;

export interface LiveProbe {
  volts: number | null;
  reading: string;
  stability: ProbeStability;
  ageSeconds: number | null;
  sampled: boolean;
}

export interface PhSectionProps {
  register: CalibrationRegister;
  errors: FieldErrors<CalibrationFormValues>;
  live: LiveProbe;
  electrodeHealth: number | null;
  onCapture: (field: "neutralVolts" | "acidVolts") => void;
}

export interface TdsSectionProps {
  register: CalibrationRegister;
  errors: FieldErrors<CalibrationFormValues>;
  live: LiveProbe;
  reportedPpm: number | null;
  onReferenceEntered: (referencePpm: string) => void;
}
