import { useEffect, useState } from "react";
import { localClockFrom, type LocalClock } from "./local-clock";

const MINUTE_MS = 60_000;

export function useLocalClock(): LocalClock {
  const [clock, setClock] = useState(() => localClockFrom(new Date()));

  useEffect(() => {
    const timer = setInterval(
      () => setClock(localClockFrom(new Date())),
      MINUTE_MS,
    );

    return () => clearInterval(timer);
  }, []);

  return clock;
}
