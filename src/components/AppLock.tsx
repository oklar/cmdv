import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import appIcon from "../assets/icon.png";

interface AppLockProps {
  onUnlock: () => void;
}

export function AppLock({ onUnlock }: AppLockProps) {
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [mnemonic, setMnemonic] = useState("");

  const handleRecover = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!mnemonic.trim()) {
      setError("Recovery phrase is required");
      return;
    }

    setError("");
    setLoading(true);
    try {
      await invoke("recover_vault", { mnemonicWords: mnemonic.trim() });
      onUnlock();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-zinc-950 flex flex-col">
      <div data-tauri-drag-region className="flex items-center justify-between px-4 py-2 border-b border-zinc-800 bg-zinc-900/80 backdrop-blur-sm">
        <span className="text-sm font-medium text-zinc-400 pointer-events-none select-none">CMDV</span>
        <button
          onClick={() => invoke("hide_to_tray")}
          className="p-2 rounded-md hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
          title="Hide to tray"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
      <div className="flex-1 flex items-center justify-center p-6">
      <div className="max-w-sm w-full space-y-6">
        <div className="text-center">
          <img src={appIcon} alt="Cmdv" className="w-12 h-12 mx-auto mb-3" />
          <h1 className="text-lg font-semibold text-zinc-100">
            Restore your vault
          </h1>
          <p className="text-sm text-zinc-500 mt-1">
            This device's key is unavailable. Enter your 24-word recovery phrase
            to restore access.
          </p>
        </div>

        <form onSubmit={handleRecover} className="space-y-4">
          <textarea
            value={mnemonic}
            onChange={(e) => setMnemonic(e.target.value)}
            placeholder="Enter your 24-word recovery phrase, separated by spaces"
            rows={4}
            autoFocus
            className="w-full bg-zinc-900 border border-zinc-800 rounded-md px-3 py-2.5 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-lime-500 focus:border-lime-500 text-sm font-mono resize-none"
          />

          {error && (
            <p className="text-red-400 text-xs text-center">{error}</p>
          )}

          <button
            type="submit"
            disabled={loading}
            className="w-full py-2.5 bg-lime-600 hover:bg-lime-500 disabled:bg-zinc-900 disabled:text-zinc-700 text-white font-medium rounded-md transition-colors"
          >
            {loading ? "Restoring..." : "Restore & Unlock"}
          </button>
        </form>
      </div>
      </div>
    </div>
  );
}
