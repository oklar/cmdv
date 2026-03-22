import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { disable, enable } from "@tauri-apps/plugin-autostart";
import { QrPairing } from "./QrPairing";

interface AppSettings {
  poll_interval_ms: number;
  max_entry_size_bytes: number;
  max_total_size_bytes: number;
  sync_interval_secs: number;
  webp_quality: number;
  excluded_apps: string[];
  mode: "local" | "cloud";
  require_password_on_open: boolean;
  login_autostart: boolean;
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
        <div className="text-zinc-500 text-sm">Loading settings...</div>
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
    <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4">
      {stats && (
        <section>
          <h2 className="text-sm font-medium text-zinc-300 mb-2">Storage</h2>
          <div className="bg-zinc-900 rounded-md p-3 space-y-2">
            <div className="flex justify-between text-xs text-zinc-400">
              <span>{stats.total_entries} entries</span>
              <span>
                {formatBytes(stats.total_size_bytes)} /{" "}
                {formatBytes(stats.max_size_bytes)}
              </span>
            </div>
            <div className="h-1.5 bg-zinc-800 rounded-full overflow-hidden">
              <div
                className="h-full rounded-full transition-all bg-gradient-to-r from-lime-500 to-lime-400"
                style={{ width: `${Math.min(usagePercent, 100)}%` }}
              />
            </div>
          </div>
        </section>
      )}

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-2">Security</h2>
        <div className="bg-zinc-900 rounded-md divide-y divide-zinc-800">
          <SettingRow
            label="Device pairing"
            description="Transfer recovery phrase via QR code"
          >
            <button
              onClick={() => setShowQr(true)}
              className="text-xs bg-zinc-800 hover:bg-zinc-700 text-zinc-300 px-3 py-1.5 rounded-md transition-colors"
            >
              Show QR
            </button>
          </SettingRow>
          <SettingRow
            label="Require password on open"
            description="Show lock screen each time the app starts"
          >
            <button
              onClick={() =>
                saveSettings({
                  ...settings,
                  require_password_on_open: !settings.require_password_on_open,
                })
              }
              className={`relative w-10 h-5 rounded-full transition-colors ${
                settings.require_password_on_open ? "bg-lime-500" : "bg-zinc-700"
              }`}
            >
              <span
                className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                  settings.require_password_on_open ? "translate-x-5" : ""
                }`}
              />
            </button>
          </SettingRow>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-2">Startup</h2>
        <div className="bg-zinc-900 rounded-md divide-y divide-zinc-800">
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
              className={`relative w-10 h-5 rounded-full transition-colors ${
                settings.login_autostart ? "bg-lime-500" : "bg-zinc-700"
              }`}
            >
              <span
                className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                  settings.login_autostart ? "translate-x-5" : ""
                }`}
              />
            </button>
          </SettingRow>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-2">Data</h2>
        <div className="bg-zinc-900 rounded-md divide-y divide-zinc-800">
          <SettingRow label="Export" description="Save encrypted backup">
            <button
              onClick={async () => {
                const path = await save({
                  defaultPath: "cmdv-backup.bin",
                  filters: [{ name: "CMDV Backup", extensions: ["bin"] }],
                });
                if (path) {
                  try {
                    const count = await invoke<number>("export_database", { path });
                    alert(`Exported ${count} entries`);
                  } catch (err) {
                    alert(String(err));
                  }
                }
              }}
              className="text-xs bg-zinc-800 hover:bg-zinc-700 text-zinc-300 px-3 py-1.5 rounded-md transition-colors"
            >
              Export
            </button>
          </SettingRow>
          <SettingRow label="Import" description="Restore from backup">
            <button
              onClick={async () => {
                const path = await open({
                  filters: [{ name: "CMDV Backup", extensions: ["bin"] }],
                  multiple: false,
                });
                if (path) {
                  try {
                    const count = await invoke<number>("import_database", { path });
                    alert(`Imported ${count} new entries`);
                  } catch (err) {
                    alert(String(err));
                  }
                }
              }}
              className="text-xs bg-zinc-800 hover:bg-zinc-700 text-zinc-300 px-3 py-1.5 rounded-md transition-colors"
            >
              Import
            </button>
          </SettingRow>
        </div>
      </section>

      <AboutSection />

      {import.meta.env.DEV && (
        <DevSection settings={settings} onSaveSettings={saveSettings} />
      )}
    </div>
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
      <h2 className="text-sm font-medium text-zinc-300 mb-2">Developer</h2>
      <div className="bg-zinc-900 rounded-md divide-y divide-zinc-800">
        <SettingRow label="Poll interval" description="How often to check clipboard">
          <select
            value={settings.poll_interval_ms}
            onChange={(e) =>
              onSaveSettings({
                ...settings,
                poll_interval_ms: Number(e.target.value),
              })
            }
            className="bg-zinc-800 text-zinc-300 text-sm rounded-md px-2 py-1 border-0 focus:ring-1 focus:ring-zinc-600"
          >
            <option value={500}>0.5s</option>
            <option value={1000}>1s</option>
            <option value={2000}>2s</option>
            <option value={5000}>5s</option>
          </select>
        </SettingRow>
        <SettingRow label="WebP quality" description="Image compression quality">
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
          <span className="text-xs text-zinc-500 ml-2 w-8">
            {settings.webp_quality}%
          </span>
        </SettingRow>
        <div className="px-3 py-2.5 flex items-center justify-between">
          <div>
            <div className="text-sm text-zinc-300">Clear all entries</div>
            <div className="text-xs text-zinc-500">
              Delete all clipboard entries but keep vault
            </div>
          </div>
          <button
            onClick={handleClearEntries}
            className={`text-xs px-3 py-1.5 rounded-md transition-colors ${
              confirming === "clear"
                ? "bg-red-600 hover:bg-red-500 text-white"
                : "bg-zinc-800 hover:bg-zinc-700 text-zinc-300"
            }`}
          >
            {confirming === "clear" ? "Confirm" : "Clear"}
          </button>
        </div>
        <div className="px-3 py-2.5 flex items-center justify-between">
          <div>
            <div className="text-sm text-red-400">Reset vault</div>
            <div className="text-xs text-zinc-500">
              Wipe everything — DB, keys, keychain. App will restart.
            </div>
          </div>
          <button
            onClick={handleReset}
            className={`text-xs px-3 py-1.5 rounded-md transition-colors ${
              confirming === "reset"
                ? "bg-red-600 hover:bg-red-500 text-white"
                : "bg-zinc-800 hover:bg-zinc-700 text-red-400"
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
    "idle" | "checking" | "downloading" | "up-to-date" | "error"
  >("idle");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const checkForUpdate = async () => {
    setUpdateStatus("checking");
    try {
      const update = await check();
      if (!update) {
        setUpdateStatus("up-to-date");
        return;
      }
      setUpdateStatus("downloading");
      await update.downloadAndInstall();
      await relaunch();
    } catch {
      setUpdateStatus("error");
    }
  };

  const statusLabel: Record<typeof updateStatus, string> = {
    idle: "Check for updates",
    checking: "Checking...",
    downloading: "Updating...",
    "up-to-date": "Up to date",
    error: "Update failed",
  };

  return (
    <section>
      <h2 className="text-sm font-medium text-zinc-300 mb-2">About</h2>
      <div className="bg-zinc-900 rounded-md divide-y divide-zinc-800">
        <SettingRow label="Version" description={version || "..."}>
          <button
            onClick={checkForUpdate}
            disabled={updateStatus === "checking" || updateStatus === "downloading"}
            className={`text-xs px-3 py-1.5 rounded-md transition-colors ${
              updateStatus === "up-to-date"
                ? "bg-lime-900/40 text-lime-400"
                : updateStatus === "error"
                  ? "bg-red-900/40 text-red-400"
                  : "bg-zinc-800 hover:bg-zinc-700 text-zinc-300"
            }`}
          >
            {statusLabel[updateStatus]}
          </button>
        </SettingRow>
        <div className="px-3 py-2 text-xs text-zinc-500">
          The app will relaunch if an update is installed.
        </div>
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
    <div className="flex items-center justify-between px-3 py-2.5">
      <div>
        <div className="text-sm text-zinc-300">{label}</div>
        <div className="text-xs text-zinc-500">{description}</div>
      </div>
      <div className="flex items-center">{children}</div>
    </div>
  );
}
