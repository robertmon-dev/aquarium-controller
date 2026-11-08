import { useEffect, useRef, type RefObject } from "react";
import uPlot from "uplot";

const FALLBACK_WIDTH = 600;

export interface ChartSpec {
  options: uPlot.Options;
  data: uPlot.AlignedData;
}

export function useUplot(
  build: (width: number) => ChartSpec | null,
  deps: unknown[],
): RefObject<HTMLDivElement | null> {
  const host = useRef<HTMLDivElement>(null);
  const builder = useRef(build);
  builder.current = build;

  useEffect(() => {
    const node = host.current;
    if (!node) return;

    const spec = builder.current(node.clientWidth || FALLBACK_WIDTH);
    if (!spec) return;

    const chart = new uPlot(spec.options, spec.data, node);

    const resize = () =>
      chart.setSize({
        width: node.clientWidth || FALLBACK_WIDTH,
        height: spec.options.height,
      });

    window.addEventListener("resize", resize);

    return () => {
      window.removeEventListener("resize", resize);
      chart.destroy();
    };
  }, deps);

  return host;
}
