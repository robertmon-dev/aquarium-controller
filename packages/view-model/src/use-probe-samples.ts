import { useRef, useState } from "react";
import { appendSample } from "./probe-calibration";

export interface ProbeVoltageReading {
  ph: number | null;
  tds: number | null;
  sampled_at_epoch_s: number;
}

export interface ProbeSamples {
  ph: number[];
  tds: number[];
}

export function useProbeSamples(
  reading: ProbeVoltageReading | undefined,
): ProbeSamples {
  const lastStamp = useRef<number | null>(null);
  const [samples, setSamples] = useState<ProbeSamples>({ ph: [], tds: [] });

  if (
    reading !== undefined &&
    reading.sampled_at_epoch_s > 0 &&
    reading.sampled_at_epoch_s !== lastStamp.current
  ) {
    lastStamp.current = reading.sampled_at_epoch_s;

    setSamples((previous) => ({
      ph: appendSample(previous.ph, reading.ph),
      tds: appendSample(previous.tds, reading.tds),
    }));
  }

  return samples;
}
