import { useCallback, useEffect, useState } from "react";
import { getAccountState } from "@/lib/tauri";
import type { AccountState } from "@/lib/types";

export function useAccountState() {
  const [account, setAccount] = useState<AccountState | null>(null);

  const refresh = useCallback(() => {
    getAccountState().then(setAccount).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { account, refresh };
}
