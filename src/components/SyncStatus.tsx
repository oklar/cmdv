import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SyncStatusData {
  is_syncing: boolean;
  last_sync_at: string | null;
  syncs_remaining: number | null;
  error: string | null;
}

interface AuthStatus {
  is_authenticated: boolean;
  email: string | null;
  has_subscription: boolean;
}

export function SyncStatus() {
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [sync, setSync] = useState<SyncStatusData | null>(null);

  useEffect(() => {
    invoke<AuthStatus>("get_auth_status").then(setAuth).catch(() => {});

    const interval = setInterval(() => {
      invoke<AuthStatus>("get_auth_status").then(setAuth).catch(() => {});
    }, 30000);
    return () => clearInterval(interval);
  }, []);

  if (!auth || !auth.is_authenticated) {
    return (
      <div className="flex items-center gap-1.5 text-xs text-zinc-500">
        <div className="w-2 h-2 rounded-full bg-emerald-500" />
        <span>Local</span>
      </div>
    );
  }

  if (!auth.has_subscription) {
    return (
      <div className="flex items-center gap-1.5 text-xs text-zinc-500">
        <div className="w-2 h-2 rounded-full bg-yellow-500" />
        <span>No subscription</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1.5 text-xs text-zinc-500">
      <div className="w-2 h-2 rounded-full bg-blue-500" />
      <span>Cloud</span>
      {sync?.syncs_remaining !== null && sync?.syncs_remaining !== undefined && (
        <span className="text-zinc-600">({sync.syncs_remaining} left)</span>
      )}
    </div>
  );
}
