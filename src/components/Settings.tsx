import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save, open } from "@tauri-apps/plugin-dialog";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { disable, enable } from "@tauri-apps/plugin-autostart";
import { QrPairing } from "./QrPairing";
import { AccountPanel } from "./AccountPanel";

interface AppSettings {
  poll_interval_ms: number;
  max_entry_size_bytes: number;
  max_total_size_bytes: number;
  webp_quality: number;
  excluded_apps: string[];
  login_autostart: boolean;
}

interface SyncStatus {
  // true = paid/entitled, false = not entitled, null = not yet checked
  entitled: boolean | null;
  syncing: boolean;
  last_synced_at: string | null;
  last_error: string | null;
}

export function Settings() {
  const [showQr, setShowQr] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [stats, setStats] = useState<{
    total_entries: number;
    total_size_bytes: number;
    max_size_bytes: number;
  } | null>(null);
  useEffect(() => {
    invoke<AppSettings>("get_settings").then(setSettings).catch(console.error);
    invoke<typeof stats>("get_stats").then(setStats).catch(console.error);
  }, []);

  const saveSettings = async (updated: AppSettings) => {
    try {
      await invoke("update_settings", { settings: updated });
      setSettings(updated);
    } catch (err) {
      console.error("Failed to save settings:", err);
    }
  };

  if (showQr) {
    return <QrPairing onClose={() => setShowQr(false)} />;
  }

  if (!settings) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-sm text-muted-foreground">Loading settings...</div>
      </div>
    );
  }

  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const usagePercent = stats
    ? (stats.total_size_bytes / stats.max_size_bytes) * 100
    : 0;

  return (
    <div className="flex-1 space-y-5 overflow-y-auto px-3 py-4">
      <AccountPanel />

      <CloudSyncSection />

      {stats && (
        <section>
          <h2 className="section-label">Storage</h2>
          <div className="space-y-2 rounded-lg border border-border bg-background/50 p-3">
            <div className="flex justify-between font-mono text-[11px] text-muted-foreground">
              <span>{stats.total_entries} entries</span>
              <span>
                {formatBytes(stats.total_size_bytes)} /{" "}
                {formatBytes(stats.max_size_bytes)}
              </span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
              <div
                className="h-full rounded-full bg-primary transition-all"
                style={{ width: `${Math.min(usagePercent, 100)}%` }}
              />
            </div>
          </div>
        </section>
      )}

      <section>
        <h2 className="section-label">Security</h2>
        <div className="group-card">
          <SettingRow
            label="Device pairing"
            description="Transfer recovery phrase via QR code"
          >
            <button
              onClick={() => setShowQr(true)}
              className="btn btn-sm btn-outline"
            >
              Show QR
            </button>
          </SettingRow>
        </div>
      </section>

      <section>
        <h2 className="section-label">Startup</h2>
        <div className="group-card">
          <SettingRow
            label="Open at login (minimized to tray)"
            description="Register Cmdv to start when you sign in; opens in the tray with a notification"
          >
            <button
              type="button"
              onClick={async () => {
                try {
                  const next = !settings.login_autostart;
                  if (!import.meta.env.DEV) {
                    if (next) {
                      await enable();
                    } else {
                      await disable();
                    }
                  }
                  await saveSettings({ ...settings, login_autostart: next });
                } catch (err) {
                  console.error("Autostart failed:", err);
                }
              }}
              role="switch"
              aria-checked={settings.login_autostart}
              className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
                settings.login_autostart ? "bg-primary" : "bg-input"
              }`}
            >
              <span
                className={`absolute top-0.5 left-0.5 size-4 rounded-full transition-transform ${
                  settings.login_autostart
                    ? "translate-x-4 bg-primary-foreground"
                    : "bg-muted-foreground"
                }`}
              />
            </button>
          </SettingRow>
        </div>
      </section>

      <section>
        <h2 className="section-label">Data</h2>
        <div className="group-card">
          <SettingRow label="Export" description="Save encrypted backup">
            <button
              onClick={async () => {
                const path = await save({
                  defaultPath: "cmdv-backup.bin",
                  filters: [{ name: "CMDV Backup", extensions: ["bin"] }],
                });
                if (path) {
                  try {
                    const count = await invoke<number>("export_database", {
                      path,
                    });
                    alert(`Exported ${count} entries`);
                  } catch (err) {
                    alert(String(err));
                  }
                }
              }}
              className="btn btn-sm btn-outline"
            >
              Export
            </button>
          </SettingRow>
          <SettingRow
            label="Import"
            description="Restore from backup using the same recovery phrase"
          >
            <button
              onClick={async () => {
                const path = await open({
                  filters: [{ name: "CMDV Backup", extensions: ["bin"] }],
                  multiple: false,
                });
                if (path) {
                  try {
                    const count = await invoke<number>("import_database", {
                      path,
                    });
                    alert(`Imported ${count} new entries`);
                  } catch (err) {
                    alert(String(err));
                  }
                }
              }}
              className="btn btn-sm btn-outline"
            >
              Import
            </button>
          </SettingRow>
        </div>
      </section>

      <AboutSection />

      <KeyboardShortcuts />

      {import.meta.env.DEV && (
        <DevSection settings={settings} onSaveSettings={saveSettings} />
      )}
    </div>
  );
}

function formatSyncedAt(millis: string | null): string {
  if (!millis) return "Never synced";
  const ms = Number(millis);
  if (!Number.isFinite(ms) || ms <= 0) return "Never synced";
  const date = new Date(ms);
  const now = Date.now();
  const diffSec = Math.round((now - ms) / 1000);
  if (diffSec < 60) return "Synced just now";
  if (diffSec < 3600) return `Synced ${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `Synced ${Math.floor(diffSec / 3600)}h ago`;
  return `Synced ${date.toLocaleDateString()}`;
}

function CloudSyncSection() {
  const [loggedIn, setLoggedIn] = useState(false);
  const [status, setStatus] = useState<SyncStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unsubs: Array<() => void> = [];

    invoke<{ logged_in: boolean }>("get_account_status")
      .then((s) => !cancelled && setLoggedIn(s.logged_in))
      .catch(() => {});

    invoke<SyncStatus>("get_sync_status")
      .then((s) => !cancelled && setStatus(s))
      .catch(() => {});

    void listen<SyncStatus>("sync-status", (event) => {
      setStatus(event.payload);
    }).then((fn) => unsubs.push(fn));

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);

  const syncing = status?.syncing ?? false;
  const lastError = status?.last_error ?? null;
  const entitled = status?.entitled ?? null;

  const handleSyncNow = async () => {
    try {
      const s = await invoke<SyncStatus>("sync_now");
      setStatus(s);
    } catch (err) {
      setStatus((prev) => ({
        entitled: prev?.entitled ?? null,
        syncing: false,
        last_synced_at: prev?.last_synced_at ?? null,
        last_error: String(err),
      }));
    }
  };

  const description = !loggedIn
    ? "Log in to use cloud sync"
    : entitled === false
      ? "Cloud sync is a paid feature"
      : formatSyncedAt(status?.last_synced_at ?? null);

  return (
    <section>
      <h2 className="section-label">Cloud Sync</h2>
      <div className="group-card">
        <SettingRow label="Automatic backup" description={description}>
          <button
            onClick={handleSyncNow}
            disabled={!loggedIn || syncing}
            className="btn btn-sm btn-outline"
          >
            {syncing ? "Syncing…" : "Sync now"}
          </button>
        </SettingRow>

        {lastError && (
          <div className="px-3 py-2 text-xs text-destructive">{lastError}</div>
        )}
      </div>
    </section>
  );
}

const SHORTCUTS = [
  { keys: "Ctrl + U", description: "Open / focus Cmdv" },
  {
    keys: "Ctrl + Shift + C",
    description:
      "Select text, then Ctrl+Shift+C to create a one-time secure link",
  },
  { keys: "Ctrl + 1–9", description: "Quick-paste entry by position" },
  { keys: "Ctrl + 0", description: "Quick-paste 10th entry" },
  { keys: "Enter", description: "Paste selected entry" },
  { keys: "↑ / ↓", description: "Navigate entries" },
  { keys: "Escape", description: "Hide to tray" },
  { keys: "Alt + A", description: "Filter: All" },
  { keys: "Alt + T", description: "Filter: Text" },
  { keys: "Alt + I", description: "Filter: Image" },
  { keys: "Alt + F", description: "Toggle favorites filter" },
  { keys: "Alt + S", description: "Star / unstar selected entry" },
] as const;

function KeyboardShortcuts() {
  return (
    <section>
      <h2 className="section-label">
        Keyboard Shortcuts
      </h2>
      <div className="group-card">
        {SHORTCUTS.map(({ keys, description }) => (
          <div
            key={keys}
            className="flex items-center justify-between gap-3 px-3 py-2"
          >
            <span className="text-xs text-muted-foreground">{description}</span>
            <span className="flex shrink-0 items-center gap-1">
              {keys.split(" + ").map((key) => (
                <kbd key={key} className="kbd">
                  {key}
                </kbd>
              ))}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

function DevSection({
  settings,
  onSaveSettings,
}: {
  settings: AppSettings;
  onSaveSettings: (updated: AppSettings) => Promise<void>;
}) {
  const [confirming, setConfirming] = useState<string | null>(null);

  const handleReset = async () => {
    if (confirming !== "reset") {
      setConfirming("reset");
      return;
    }
    try {
      await invoke("reset_vault");
    } catch (err) {
      alert(String(err));
      setConfirming(null);
    }
  };

  const handleClearEntries = async () => {
    if (confirming !== "clear") {
      setConfirming("clear");
      return;
    }
    try {
      await invoke("clear_all_entries");
      alert("All entries cleared");
      setConfirming(null);
    } catch (err) {
      alert(String(err));
      setConfirming(null);
    }
  };

  return (
    <section>
      <h2 className="section-label">Developer</h2>
      <div className="group-card">
        <SettingRow
          label="Poll interval"
          description="How often to check clipboard"
        >
          <select
            value={settings.poll_interval_ms}
            onChange={(e) =>
              onSaveSettings({
                ...settings,
                poll_interval_ms: Number(e.target.value),
              })
            }
            className="rounded-md border border-border bg-secondary px-2 py-1 text-xs text-foreground"
          >
            <option value={500}>0.5s</option>
            <option value={1000}>1s</option>
            <option value={2000}>2s</option>
            <option value={5000}>5s</option>
          </select>
        </SettingRow>
        <SettingRow
          label="WebP quality"
          description="Image compression quality"
        >
          <input
            type="range"
            min="50"
            max="100"
            value={settings.webp_quality}
            onChange={(e) =>
              onSaveSettings({
                ...settings,
                webp_quality: Number(e.target.value),
              })
            }
            className="w-24"
          />
          <span className="ml-2 w-8 font-mono text-[11px] text-muted-foreground">
            {settings.webp_quality}%
          </span>
        </SettingRow>
        <div className="flex items-center justify-between gap-3 px-3 py-2.5">
          <div>
            <div className="text-sm text-foreground">Clear all entries</div>
            <div className="text-xs text-muted-foreground">
              Delete all clipboard entries but keep vault
            </div>
          </div>
          <button
            onClick={handleClearEntries}
            className={`btn btn-sm shrink-0 ${
              confirming === "clear" ? "btn-danger" : "btn-outline"
            }`}
          >
            {confirming === "clear" ? "Confirm" : "Clear"}
          </button>
        </div>
        <div className="flex items-center justify-between gap-3 px-3 py-2.5">
          <div>
            <div className="text-sm text-destructive">Reset vault</div>
            <div className="text-xs text-muted-foreground">
              Wipe everything: DB, keys, keychain. App will restart.
            </div>
          </div>
          <button
            onClick={handleReset}
            className={`btn btn-sm shrink-0 ${
              confirming === "reset"
                ? "btn-danger"
                : "btn-outline text-destructive"
            }`}
          >
            {confirming === "reset" ? "Confirm reset" : "Reset"}
          </button>
        </div>
      </div>
    </section>
  );
}

function AboutSection() {
  const [version, setVersion] = useState("");
  const [updateStatus, setUpdateStatus] = useState<
    "idle" | "checking" | "no-update" | "updating" | "error"
  >("idle");

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);

  const checkForUpdate = async () => {
    setUpdateStatus("checking");
    try {
      const update = await check();
      if (!update) {
        setUpdateStatus("no-update");
        return;
      }
      setUpdateStatus("updating");
      await update.downloadAndInstall();
      await relaunch();
    } catch {
      setUpdateStatus("no-update");
    }
  };

  const busy = updateStatus === "checking" || updateStatus === "updating";

  const label =
    updateStatus === "checking" ? "Checking..." :
    updateStatus === "updating" ? "Updating..." :
    updateStatus === "no-update" ? "No update" :
    updateStatus === "error" ? "Try again later" :
    "Check for updates";

  return (
    <section>
      <h2 className="section-label">About</h2>
      <div className="group-card">
        <SettingRow label="Version" description={version || "..."}>
          <button
            onClick={checkForUpdate}
            disabled={busy}
            className={`btn btn-sm ${
              busy
                ? "btn-outline"
                : updateStatus === "no-update"
                  ? "border border-primary/30 bg-primary/10 text-primary"
                  : updateStatus === "error"
                    ? "border border-destructive/30 bg-destructive/10 text-destructive"
                    : "btn-outline"
            }`}
          >
            {label}
          </button>
        </SettingRow>
      </div>
    </section>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3 px-3 py-2.5">
      <div className="min-w-0">
        <div className="text-sm text-foreground">{label}</div>
        <div className="text-xs text-muted-foreground">{description}</div>
      </div>
      <div className="flex shrink-0 items-center">{children}</div>
    </div>
  );
}
