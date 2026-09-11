import type { RequestLog } from "../services/gateway";

export const LIVE_REFRESH_WINDOW_MS = 1000;

export type LiveSignal = {
  revision: number;
  lastLog: RequestLog | null;
};

export const EMPTY_LIVE_SIGNAL: LiveSignal = { revision: 0, lastLog: null };

export function mergeLiveSignal(prev: LiveSignal, log: RequestLog): LiveSignal {
  return { revision: prev.revision + 1, lastLog: log };
}
