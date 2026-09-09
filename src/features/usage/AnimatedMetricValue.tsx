import { useAnimatedMetric } from "./useAnimatedMetric";

type AnimatedMetricValueProps = {
  target: string;
  className?: string;
};

export function AnimatedMetricValue({
  target,
  className,
}: AnimatedMetricValueProps) {
  const display = useAnimatedMetric(target);
  return (
    <span className={className} aria-label={target}>
      {display}
    </span>
  );
}
