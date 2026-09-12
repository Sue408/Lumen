import { Brain, Eye, Wrench, type LucideIcon } from "lucide-react";
import type { CapabilityId } from "./providerModel";

export const capabilityIcons: Record<CapabilityId, LucideIcon> = {
  vision: Eye,
  tools: Wrench,
  reasoning: Brain,
};
