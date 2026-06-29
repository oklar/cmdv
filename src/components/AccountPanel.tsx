import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

interface AccountStatus {
  logged_in: boolean;
  email: string | null;
}

interface AccountInfo {
  email: string;
}

export function AccountPanel() {
  const [email, setEmail] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const verifyAccount = useCallback(async () => {
    try {
      const info = await invoke<AccountInfo>("fetch_account");
      setEmail(info.email);
      setError(null);
    } catch (err) {
      // Token could not be validated/refreshed: treat as logged out.
      setEmail(null);
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unsubs: Array<() => void> = [];

    invoke<AccountStatus>("get_account_status")
      .then((status) => {
        if (cancelled) return;
        if (status.logged_in) {
          setEmail(status.email);
          void verifyAccount();
        }
      })
      .catch(() => {});

    void listen<AccountInfo>("desktop-auth-success", (event) => {
      setLoading(false);
      setError(null);
      setEmail(event.payload.email);
    }).then((fn) => unsubs.push(fn));

    void listen<string>("desktop-auth-error", (event) => {
      setLoading(false);
      setError(event.payload);
    }).then((fn) => unsubs.push(fn));

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, [verifyAccount]);

  const handleLogin = async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke("begin_desktop_login");
    } catch (err) {
      setLoading(false);
      setError(String(err));
    }
  };

  const handleLogout = async () => {
    try {
      await invoke("desktop_logout");
      setEmail(null);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <section>
      <h2 className="text-sm font-medium text-zinc-300 mb-2">Account</h2>
      <div className="bg-zinc-900 rounded-md divide-y divide-zinc-800">
        {email ? (
          <div className="flex items-center justify-between gap-3 px-3 py-2.5">
            <div className="min-w-0">
              <div className="truncate text-sm text-zinc-300">{email}</div>
              <div className="text-xs text-zinc-500">Signed in to your cmdv account</div>
            </div>
            <button
              onClick={handleLogout}
              className="shrink-0 whitespace-nowrap text-xs bg-zinc-800 hover:bg-zinc-700 text-zinc-300 px-3 py-1.5 rounded-md transition-colors"
            >
              Log out
            </button>
          </div>
        ) : (
          <div className="flex items-center justify-between gap-3 px-3 py-2.5">
            <div className="min-w-0">
              <div className="text-sm text-zinc-300">Not signed in</div>
              <div className="text-xs text-zinc-500">
                Log in through your browser to connect this device
              </div>
            </div>
            <button
              onClick={handleLogin}
              disabled={loading}
              className={`shrink-0 whitespace-nowrap text-xs px-3 py-1.5 rounded-md transition-colors ${
                loading
                  ? "bg-zinc-800 text-zinc-500 cursor-not-allowed"
                  : "bg-lime-600 hover:bg-lime-500 text-white"
              }`}
            >
              {loading ? "Waiting for browser…" : "Log in"}
            </button>
          </div>
        )}
        {error && (
          <div className="px-3 py-2 text-xs text-red-400">{error}</div>
        )}
      </div>
    </section>
  );
}
