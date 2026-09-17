import { useEffect, useState } from "react";
import { getConnectivity, onConnectivityChanged } from "@/lib/tauri";
import type { ConnectivityState } from "@/lib/types";

/**
 * Tracks live connectivity state. Reads the current value once on mount
 * (the backend already knows it from its low-frequency background poll)
 * and then just listens for `connectivity-changed` -- no polling from
 * the frontend, matching the spec's "do not repeatedly attempt network
 * requests in the background" rule.
 */
export function useConnectivity(): ConnectivityState {
  const [state, setState] = useState<ConnectivityState>("online");

  useEffect(() => {
    let cancelled = false;
    getConnectivity().then((s) => {
      if (!cancelled) setState(s);
    });

    const unlistenPromise = onConnectivityChanged((s) => setState(s));
    return () => {
      cancelled = true;
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  return state;
}
