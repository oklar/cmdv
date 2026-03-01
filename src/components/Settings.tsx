import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import { QrPairing } from "./QrPairing";

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
        <div className="bg-zinc-900 rounded-lg p-4 space-y-3">
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
          <p className="text-xs text-zinc-500">
            {settings.mode === "cloud"
              ? "Your clipboard syncs across devices"
              : "All data stays on this device"}
          </p>
          {settings.mode === "local" ? (
            <button
              onClick={async () => {
                try {
                  await invoke("switch_to_cloud");
                  setSettings({ ...settings, mode: "cloud" });
                } catch (err) {
                  alert(String(err));
                }
              }}
              className="w-full py-2 px-4 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded-lg transition-colors"
            >
              Enable Cloud Sync
            </button>
          ) : (
            <button
              onClick={async () => {
                try {
                  await invoke("switch_to_local");
                  setSettings({ ...settings, mode: "local" });
                } catch (err) {
                  alert(String(err));
                }
              }}
              className="w-full py-2 px-4 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm rounded-lg transition-colors"
            >
              Switch to Local Only
            </button>
          )}
        </div>
      </section>

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-3">Data</h2>
        <div className="bg-zinc-900 rounded-lg divide-y divide-zinc-800">
          <SettingRow label="Export" description="Save encrypted backup">
            <button
              onClick={async () => {
                const path = await save({
                  defaultPath: "cmdv-backup.bin",
                  filters: [{ name: "CMD Backup", extensions: ["bin"] }],
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
                  filters: [{ name: "CMD Backup", extensions: ["bin"] }],
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

      <section>
        <h2 className="text-sm font-medium text-zinc-300 mb-3">Account</h2>
        <div className="bg-zinc-900 rounded-lg p-4 space-y-3">
          <AccountSection />
        </div>
      </section>
    </div>
  );
}

function AccountSection() {
  const [auth, setAuth] = useState<{
    is_authenticated: boolean;
    email: string | null;
    has_subscription: boolean;
  } | null>(null);
  const [loginForm, setLoginForm] = useState({ email: "", password: "" });
  const [showLogin, setShowLogin] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<typeof auth>("get_auth_status").then(setAuth).catch(() => {});
  }, []);

  if (!auth) return <div className="text-sm text-zinc-500">Loading...</div>;

  if (auth.is_authenticated) {
    return (
      <div className="space-y-2">
        <div className="text-sm text-zinc-300">{auth.email}</div>
        <div className="text-xs text-zinc-500">
          {auth.has_subscription ? "Active subscription" : "No subscription"}
        </div>
        <button
          onClick={async () => {
            await invoke("logout");
            setAuth({ is_authenticated: false, email: null, has_subscription: false });
          }}
          className="text-xs text-red-400 hover:text-red-300"
        >
          Sign out
        </button>
      </div>
    );
  }

  if (!showLogin) {
    return (
      <button
        onClick={() => setShowLogin(true)}
        className="text-sm text-blue-400 hover:text-blue-300"
      >
        Sign in for cloud sync
      </button>
    );
  }

  return (
    <div className="space-y-3">
      {error && <div className="text-xs text-red-400">{error}</div>}
      <input
        type="email"
        placeholder="Email"
        value={loginForm.email}
        onChange={(e) => setLoginForm({ ...loginForm, email: e.target.value })}
        className="w-full bg-zinc-800 text-zinc-300 text-sm rounded-md px-3 py-2 border border-zinc-700 focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
      />
      <input
        type="password"
        placeholder="Password"
        value={loginForm.password}
        onChange={(e) => setLoginForm({ ...loginForm, password: e.target.value })}
        className="w-full bg-zinc-800 text-zinc-300 text-sm rounded-md px-3 py-2 border border-zinc-700 focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
      />
      <div className="flex gap-2">
        <button
          onClick={async () => {
            try {
              setError(null);
              await invoke("login", loginForm);
              const status = await invoke<typeof auth>("get_auth_status");
              setAuth(status);
              setShowLogin(false);
            } catch (err) {
              setError(String(err));
            }
          }}
          className="flex-1 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded-lg transition-colors"
        >
          Sign in
        </button>
        <button
          onClick={async () => {
            try {
              setError(null);
              await invoke("register", loginForm);
              const status = await invoke<typeof auth>("get_auth_status");
              setAuth(status);
              setShowLogin(false);
            } catch (err) {
              setError(String(err));
            }
          }}
          className="flex-1 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm rounded-lg transition-colors"
        >
          Register
        </button>
      </div>
      <button
        onClick={() => setShowLogin(false)}
        className="text-xs text-zinc-500 hover:text-zinc-400"
      >
        Cancel
      </button>
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
