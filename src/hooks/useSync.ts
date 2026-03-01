import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

type SyncState = "idle" | "syncing" | "error" | "offline";

interface SyncStatus {
  is_syncing: boolean;
  last_sync_at: string | null;
  syncs_remaining: number | null;
  error: string | null;
}

interface SyncResult {
  success: boolean;
  entries_merged: number;
  error: string | null;
}

export function useSync() {
  const [state, setState] = useState<SyncState>("idle");
  const [lastSyncAt, setLastSyncAt] = useState<string | null>(null);
  const [syncsRemaining, setSyncsRemaining] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const triggerSync = useCallback(async () => {
    setState("syncing");
    setError(null);
    try {
      const result = await invoke<SyncResult>("trigger_sync");
      if (result.success) {
        setState("idle");
        setLastSyncAt(new Date().toISOString());
      } else {
        setState("error");
        setError(result.error || "Sync failed");
      }
    } catch (err) {
      setState("error");
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await invoke<SyncStatus>("get_sync_status");
      setLastSyncAt(status.last_sync_at);
      setSyncsRemaining(status.syncs_remaining);
      if (status.error) {
        setError(status.error);
      }
    } catch {
      // Offline or not authenticated
    }
  }, []);

  return {
    state,
    lastSyncAt,
    syncsRemaining,
    error,
    triggerSync,
    refreshStatus,
  };
}
