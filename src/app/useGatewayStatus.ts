import { useCallback, useEffect, useState } from "react";
import {
  fetchGatewayStatus,
  onGatewayStatus,
  startGateway,
  stopGateway,
  type GatewayStatus,
} from "../services/gateway";

export type GatewayController = {
  status: GatewayStatus | null;
  busy: boolean;
  error: string | null;
  toggle: () => void;
};

export function useGatewayStatus(): GatewayController {
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    void fetchGatewayStatus()
      .then((next) => {
        if (active) setStatus(next);
      })
      .catch((reason: unknown) => {
        if (active) setError(String(reason));
      });

    void onGatewayStatus((next) => {
      if (!active) return;
      setStatus(next);
      setError(next.error);
    }).then((cleanup) => {
      unlisten = cleanup;
    });

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const toggle = useCallback(() => {
    setBusy(true);
    setError(null);
    const action = status?.running ? stopGateway : startGateway;
    void action()
      .then((next) => {
        setStatus(next);
        setError(next.error);
      })
      .catch((reason: unknown) => {
        setError(String(reason));
      })
      .finally(() => setBusy(false));
  }, [status?.running]);

  return { status, busy, error, toggle };
}
