import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AppSettings {
  poll_interval_ms: number;
  max_entry_size_bytes: number;
  max_total_size_bytes: number;
  sensitive_auto_expire_secs: number;
  sync_interval_secs: number;
  webp_quality: number;
  excluded_apps: string[];
  sync_sensitive: boolean;
  mode: "local" | "cloud";
}

export function Settings() {
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
    <div className="flex-1 overflow-y-auto px-4 py-4 space-y-6">
      {stats && (
        <section>
          <h2 className="text-sm font-medium text-zinc-300 mb-3">Storage</h2>
          <div className="bg-zinc-900 rounded-lg p-4 space-y-2">
            <div className="flex justify-between text-xs text-zinc-400">
              <span>{stats.total_entries} entries</span>
              <span>
                {formatBytes(stats.total_size_bytes)} /{" "}
                {formatBytes(stats.max_size_bytes)}
              </span>
            </div>
            <div className="h-1.5 bg-zinc-800 rounded-full overflow-hidden">
              <div
                className="h-full rounded-full transition-all bg-gradient-to-r from-blue-500 to-blue-400"
                style={{ width: `${Math.min(usagePercent, 100)}%` }}
              />
            </div>
          </div>
        </section>
      )}

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-3">General</h2>
        <div className="bg-zinc-900 rounded-lg divide-y divide-zinc-800">
          <SettingRow label="Poll interval" description="How often to check clipboard">
            <select
              value={settings.poll_interval_ms}
              onChange={(e) =>
                saveSettings({
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
                saveSettings({
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
        </div>
      </section>

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-3">Security</h2>
        <div className="bg-zinc-900 rounded-lg divide-y divide-zinc-800">
          <SettingRow
            label="Sensitive auto-expire"
            description="Auto-delete sensitive entries after"
          >
            <select
              value={settings.sensitive_auto_expire_secs}
              onChange={(e) =>
                saveSettings({
                  ...settings,
                  sensitive_auto_expire_secs: Number(e.target.value),
                })
              }
              className="bg-zinc-800 text-zinc-300 text-sm rounded-md px-2 py-1 border-0 focus:ring-1 focus:ring-zinc-600"
            >
              <option value={60}>1 min</option>
              <option value={300}>5 min</option>
              <option value={600}>10 min</option>
              <option value={1800}>30 min</option>
              <option value={0}>Never</option>
            </select>
          </SettingRow>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-3">Mode</h2>
        <div className="bg-zinc-900 rounded-lg p-4">
          <div className="flex items-center gap-3">
            <div
              className={`w-2 h-2 rounded-full ${
                settings.mode === "cloud" ? "bg-blue-400" : "bg-emerald-500"
              }`}
            />
            <span className="text-sm text-zinc-300">
              {settings.mode === "cloud" ? "Cloud sync enabled" : "Local only"}
            </span>
          </div>
          <p className="text-xs text-zinc-500 mt-2">
            {settings.mode === "cloud"
              ? "Your clipboard syncs across devices"
              : "All data stays on this device"}
          </p>
        </div>
      </section>
    </div>
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
    <div className="flex items-center justify-between px-4 py-3">
      <div>
        <div className="text-sm text-zinc-300">{label}</div>
        <div className="text-xs text-zinc-500">{description}</div>
      </div>
      <div className="flex items-center">{children}</div>
    </div>
  );
}
