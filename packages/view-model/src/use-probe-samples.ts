import { useRef, useState } from "react";
import {
  advanceProbeSamples,
  type ProbeSamples,
  type ProbeVoltageReading,
} from "./probe-calibration";

export type { ProbeSamples, ProbeVoltageReading } from "./probe-calibration";

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
    const previousStamp = lastStamp.current;
    lastStamp.current = reading.sampled_at_epoch_s;

    setSamples((previous) =>
      advanceProbeSamples(previous, reading, previousStamp),
    );
  }

  return samples;
}
