import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AppLockProps {
  onUnlock: () => void;
}

export function AppLock({ onUnlock }: AppLockProps) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [recoveryMode, setRecoveryMode] = useState(false);
  const [mnemonic, setMnemonic] = useState("");

  const handleUnlock = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password) {
      setError("Password required");
      return;
    }

    setError("");
    setLoading(true);
    try {
      await invoke("unlock_vault", { password });
      onUnlock();
    } catch (err) {
      const msg = String(err);
      if (msg.includes("NEEDS_RECOVERY")) {
        setRecoveryMode(true);
        setError(
          "Keychain unavailable. Enter your 24-word recovery phrase to restore access."
        );
      } else {
        setError(msg);
      }
    } finally {
      setLoading(false);
    }
  };

  const handleRecover = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password || !mnemonic.trim()) {
      setError("Password and recovery phrase are required");
      return;
    }

    setError("");
    setLoading(true);
    try {
      await invoke("recover_vault", {
        password,
        mnemonicWords: mnemonic.trim(),
      });
      onUnlock();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-zinc-950 flex items-center justify-center p-8">
      <div className="max-w-sm w-full space-y-8">
        <div className="text-center">
          <div className="w-16 h-16 rounded-2xl bg-zinc-900 flex items-center justify-center mx-auto mb-4">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="28"
              height="28"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              className="text-zinc-500"
            >
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
              <path d="M7 11V7a5 5 0 0 1 10 0v4" />
            </svg>
          </div>
          <h1 className="text-lg font-semibold text-zinc-100">
            CMDV is locked
          </h1>
          <p className="text-sm text-zinc-500 mt-1">
            {recoveryMode
              ? "Enter your password and recovery phrase"
              : "Enter your password to unlock"}
          </p>
        </div>

        <form
          onSubmit={recoveryMode ? handleRecover : handleUnlock}
          className="space-y-4"
        >
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password"
            autoFocus
            className="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-3 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-zinc-600 focus:border-zinc-600"
          />

          {recoveryMode && (
            <textarea
              value={mnemonic}
              onChange={(e) => setMnemonic(e.target.value)}
              placeholder="Enter your 24-word recovery phrase, separated by spaces"
              rows={4}
              className="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-3 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-zinc-600 focus:border-zinc-600 text-sm font-mono resize-none"
            />
          )}

          {error && (
            <p className="text-red-400 text-xs text-center">{error}</p>
          )}

          <button
            type="submit"
            disabled={loading}
            className="w-full py-3 bg-zinc-800 hover:bg-zinc-700 disabled:bg-zinc-900 disabled:text-zinc-700 text-zinc-200 font-medium rounded-lg transition-colors"
          >
            {loading
              ? recoveryMode
                ? "Recovering..."
                : "Unlocking..."
              : recoveryMode
                ? "Recover & Unlock"
                : "Unlock"}
          </button>
        </form>
      </div>
    </div>
  );
}
