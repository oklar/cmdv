import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MnemonicDisplay } from "./MnemonicDisplay";
import appIcon from "../assets/icon.png";

interface SetupWizardProps {
  onComplete: () => void;
}

type Step = "welcome" | "restore" | "mnemonic";

function HideToTrayButton() {
  return (
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
  );
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
  const [step, setStep] = useState<Step>("welcome");
  const [mnemonic, setMnemonic] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [mnemonicWords, setMnemonicWords] = useState<string[]>([]);

  const handleCreateVault = async () => {
    setError("");
    setLoading(true);
    try {
      const result = await invoke<{ mnemonic: string[] }>("setup_vault");
      setMnemonicWords(result.mnemonic);
      setStep("mnemonic");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleRestore = async () => {
    if (!mnemonic.trim()) {
      setError("Enter your 24-word recovery phrase");
      return;
    }
    setError("");
    setLoading(true);
    try {
      await invoke("recover_vault", { mnemonicWords: mnemonic.trim() });
      onComplete();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  if (step === "mnemonic") {
    return (
      <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col">
        <div data-tauri-drag-region className="flex items-center justify-end px-4 py-2">
          <HideToTrayButton />
        </div>
        <div className="flex-1 flex items-center justify-center p-8">
          <MnemonicDisplay words={mnemonicWords} onConfirm={onComplete} />
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col">
      <div data-tauri-drag-region className="flex items-center justify-end px-4 py-2">
        <HideToTrayButton />
      </div>
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="max-w-md w-full space-y-8">
        {step === "welcome" && (
          <div className="text-center space-y-4">
            <img src={appIcon} alt="Cmdv" className="w-20 h-20 mx-auto" />
            <div>
              <h1 className="text-2xl font-bold">Welcome to CMDV</h1>
              <p className="text-zinc-400 mt-2 text-sm">
                Your encrypted clipboard manager. Everything is encrypted
                locally with a key only you control. We'll generate that key and
                show you a 24-word recovery phrase to back it up.
              </p>
            </div>
            <button
              onClick={handleCreateVault}
              disabled={loading}
              className="w-full py-2.5 bg-lime-600 hover:bg-lime-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-white font-medium rounded-md transition-colors"
            >
              {loading ? "Creating vault..." : "Create vault"}
            </button>
            <button
              onClick={() => {
                setError("");
                setStep("restore");
              }}
              className="w-full py-2.5 text-sm text-zinc-400 hover:text-zinc-200 transition-colors"
            >
              Restore from recovery phrase
            </button>
            {error && <p className="text-red-400 text-xs">{error}</p>}
          </div>
        )}

        {step === "restore" && (
          <div className="space-y-4">
            <div>
              <h2 className="text-xl font-bold">Restore your vault</h2>
              <p className="text-zinc-400 text-sm mt-1">
                Enter the 24-word recovery phrase from your other device. This
                rebuilds your encryption key so your synced data can be
                decrypted here.
              </p>
            </div>

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

            <div className="flex gap-3">
              <button
                onClick={() => {
                  setError("");
                  setStep("welcome");
                }}
                className="flex-1 py-2.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-medium rounded-md transition-colors"
              >
                Back
              </button>
              <button
                onClick={handleRestore}
                disabled={loading}
                className="flex-1 py-2.5 bg-lime-600 hover:bg-lime-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-white font-medium rounded-md transition-colors"
              >
                {loading ? "Restoring..." : "Restore vault"}
              </button>
            </div>
          </div>
        )}
      </div>
      </div>
    </div>
  );
}
