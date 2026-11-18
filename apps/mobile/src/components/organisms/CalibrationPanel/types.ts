import type {
  CalibrationFormValues,
  ParsedCalibrationForm,
} from "@aquarium/api-client";
import type { ProbeStability } from "@aquarium/view-model";
import type { Control } from "react-hook-form";

export type CalibrationControl = Control<
  CalibrationFormValues,
  unknown,
  ParsedCalibrationForm
>;

export interface LiveProbe {
  volts: number | null;
  reading: string;
  stability: ProbeStability;
  ageSeconds: number | null;
  sampled: boolean;
}

export interface PhSectionProps {
  control: CalibrationControl;
  live: LiveProbe;
  electrodeHealth: number | null;
  onCapture: (field: "neutralVolts" | "acidVolts") => void;
}

export interface TdsSectionProps {
  control: CalibrationControl;
  live: LiveProbe;
  reportedPpm: number | null;
  onReferenceEntered: (referencePpm: string) => void;
}
