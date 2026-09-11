import { useEffect, useRef, useState } from "react";
import { onGatewayLog, type RequestLog } from "../services/gateway";
import {
  EMPTY_LIVE_SIGNAL,
  LIVE_REFRESH_WINDOW_MS,
  mergeLiveSignal,
  type LiveSignal,
} from "./liveSignal";

export function useLiveRevision(): LiveSignal {
  const [signal, setSignal] = useState<LiveSignal>(EMPTY_LIVE_SIGNAL);
  const pending = useRef<RequestLog | null>(null);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    const flush = () => {
      timer.current = null;
      if (!active) return;
      const log = pending.current;
      pending.current = null;
      if (!log) return;
      setSignal((prev) => mergeLiveSignal(prev, log));
    };

    const schedule = () => {
      if (timer.current !== null) return;
      timer.current = window.setTimeout(flush, LIVE_REFRESH_WINDOW_MS);
    };

    const handleLog = (log: RequestLog) => {
      pending.current = log;
      if (document.visibilityState !== "hidden") schedule();
    };

    const handleVisibility = () => {
      if (document.visibilityState !== "hidden" && pending.current !== null) schedule();
    };

    void onGatewayLog(handleLog).then((cleanup) => {
      if (active) unlisten = cleanup;
      else cleanup();
    });
    document.addEventListener("visibilitychange", handleVisibility);

    return () => {
      active = false;
      unlisten?.();
      document.removeEventListener("visibilitychange", handleVisibility);
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        timer.current = null;
      }
    };
  }, []);

  return signal;
}
