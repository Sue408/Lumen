import { useEffect, useState } from "react";
import { formatAnimatedMetric } from "./metricAnimation";

const duration = 360;

function prefersReducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function useAnimatedMetric(target: string) {
  const [display, setDisplay] = useState(() =>
    typeof window === "undefined" || prefersReducedMotion()
      ? target
      : formatAnimatedMetric(target, 0),
  );

  useEffect(() => {
    if (prefersReducedMotion()) {
      setDisplay(target);
      return;
    }

    let frame = 0;
    let startTime: number | undefined;
    setDisplay(formatAnimatedMetric(target, 0));

    const tick = (time: number) => {
      startTime ??= time;
      const rawProgress = Math.min((time - startTime) / duration, 1);
      const easedProgress = 1 - Math.pow(1 - rawProgress, 4);
      setDisplay(formatAnimatedMetric(target, easedProgress));
      if (rawProgress < 1) frame = requestAnimationFrame(tick);
    };

    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [target]);

  return display;
}
