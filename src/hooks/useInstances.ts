import { useCallback, useEffect, useState } from "react";
import { listInstances } from "@/lib/tauri";
import type { Instance } from "@/lib/types";

const SELECTED_KEY = "blocklight:selected-instance";

/**
 * Loads the local instance list and tracks which one is "selected" for
 * the link-import flow and offline readiness checks. Selection is just
 * UI convenience state (which instance panels/pages operate on) and is
 * remembered locally between launches.
 */
export function useInstances() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(
    () => localStorage.getItem(SELECTED_KEY)
  );
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    const list = await listInstances();
    setInstances(list);
    setSelectedId((current) => {
      if (current && list.some((i) => i.id === current)) return current;
      return list[0]?.id ?? null;
    });
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (selectedId) localStorage.setItem(SELECTED_KEY, selectedId);
  }, [selectedId]);

  const selected = instances.find((i) => i.id === selectedId) ?? null;

  return { instances, selected, selectedId, setSelectedId, loading, refresh };
}
