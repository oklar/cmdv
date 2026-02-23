import { useState, useCallback } from "react";

type SyncState = "idle" | "syncing" | "error" | "offline";

export function useSync() {
  const [state, setState] = useState<SyncState>("idle");
  const [lastSyncAt, setLastSyncAt] = useState<string | null>(null);
  const [syncsRemaining] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const triggerSync = useCallback(async () => {
    setState("syncing");
    try {
      // Sync logic connects to the API in Phase 4 integration
      setState("idle");
      setLastSyncAt(new Date().toISOString());
    } catch (err) {
      setState("error");
      setError(err instanceof Error ? err.message : "Sync failed");
    }
  }, []);

  return {
    state,
    lastSyncAt,
    syncsRemaining,
    error,
    triggerSync,
  };
}
