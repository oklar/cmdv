import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TitleBar } from "./TitleBar";
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
    <div className="flex min-h-screen flex-col bg-card text-foreground">
      <TitleBar />
      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-sm space-y-6">
          <div className="text-center">
            <img src={appIcon} alt="Cmdv" className="mx-auto mb-4 size-12" />
            <h1 className="text-lg font-semibold tracking-tight">
              Restore your vault
            </h1>
            <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
              This device's key is unavailable. Enter your 24-word recovery
              phrase to restore access.
            </p>
          </div>

          <form onSubmit={handleRecover} className="space-y-4">
            <textarea
              value={mnemonic}
              onChange={(e) => setMnemonic(e.target.value)}
              placeholder="Enter your 24-word recovery phrase, separated by spaces"
              rows={4}
              autoFocus
              className="field resize-none font-mono"
            />

            {error && (
              <p className="text-center text-xs text-destructive">{error}</p>
            )}

            <button
              type="submit"
              disabled={loading}
              className="btn btn-primary w-full"
            >
              {loading ? "Restoring..." : "Restore & Unlock"}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
