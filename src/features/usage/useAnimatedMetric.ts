import { useEffect, useRef, useState } from "react";
import { formatMetricValue, parseMetricValue } from "./metricAnimation";

const duration = 360;

function prefersReducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function useAnimatedMetric(target: string) {
  const [display, setDisplay] = useState(() =>
    typeof window === "undefined" || prefersReducedMotion()
      ? target
      : formatMetricValue(target, 0),
  );
  // 记录当前展示值：目标变化时从它补间，而不是每次都从 0 重数。
  const current = useRef(0);

  useEffect(() => {
    const to = parseMetricValue(target);
    if (prefersReducedMotion()) {
      current.current = to;
      setDisplay(target);
      return;
    }

    const from = current.current;
    let frame = 0;
    let startTime: number | undefined;

    const tick = (time: number) => {
      startTime ??= time;
      const rawProgress = Math.min((time - startTime) / duration, 1);
      const easedProgress = 1 - Math.pow(1 - rawProgress, 4);
      const value = from + (to - from) * easedProgress;
      current.current = value;
      setDisplay(formatMetricValue(target, value));
      if (rawProgress < 1) frame = requestAnimationFrame(tick);
    };

    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [target]);

  return display;
}
