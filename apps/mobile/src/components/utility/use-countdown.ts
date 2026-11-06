import { useEffect, useRef, useState } from "react";
import { tickCountdown } from "./lighting-request";

export function useCountdown(
  seconds: number | null,
  onElapsed?: () => void,
): number | null {
  const [remaining, setRemaining] = useState<number | null>(seconds);
  const elapsedCallback = useRef(onElapsed);
  elapsedCallback.current = onElapsed;

  useEffect(() => {
    setRemaining(seconds);
  }, [seconds]);

  const idle = remaining === null || remaining <= 0;

  useEffect(() => {
    if (idle) return;

    const timer = setInterval(() => {
      setRemaining((current) => {
        const { next, elapsed } = tickCountdown(current);
        if (elapsed) elapsedCallback.current?.();
        return next;
      });
    }, 1000);

    return () => clearInterval(timer);
  }, [idle]);

  return remaining;
}
