import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MnemonicDisplay } from "./MnemonicDisplay";
import { TitleBar } from "./TitleBar";
import appIcon from "../assets/icon.png";

interface SetupWizardProps {
  onComplete: () => void;
}

type Step = "welcome" | "restore" | "mnemonic";

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
      <div className="flex min-h-screen flex-col bg-card text-foreground">
        <TitleBar />
        <div className="flex flex-1 items-center justify-center p-4">
          <MnemonicDisplay words={mnemonicWords} onConfirm={onComplete} />
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen flex-col bg-card text-foreground">
      <TitleBar />
      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-md">
          {step === "welcome" && (
            <div className="space-y-6 text-center">
              <img src={appIcon} alt="Cmdv" className="mx-auto size-20" />
              <div>
                <h1 className="text-2xl font-semibold tracking-tight">
                  Welcome to CMDV
                </h1>
                <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
                  Use the same recovery phrase on every device to share data
                  through cloud sync or export/import.
                </p>
              </div>
              <div className="flex flex-col gap-2.5">
                <button
                  onClick={handleCreateVault}
                  disabled={loading}
                  className="btn btn-primary w-full"
                >
                  {loading ? "Creating..." : "Create new vault"}
                </button>
                <button
                  onClick={() => {
                    setError("");
                    setStep("restore");
                  }}
                  disabled={loading}
                  className="btn btn-outline w-full"
                >
                  Restore from recovery phrase
                </button>
              </div>
              {error && <p className="text-xs text-destructive">{error}</p>}
            </div>
          )}

          {step === "restore" && (
            <div className="space-y-4">
              <div>
                <h2 className="text-xl font-semibold tracking-tight">
                  Restore your vault
                </h2>
                <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
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
                className="field resize-none font-mono"
              />

              {error && (
                <p className="text-center text-xs text-destructive">{error}</p>
              )}

              <div className="flex gap-2.5">
                <button
                  onClick={() => {
                    setError("");
                    setStep("welcome");
                  }}
                  className="btn btn-outline flex-1"
                >
                  Back
                </button>
                <button
                  onClick={handleRestore}
                  disabled={loading}
                  className="btn btn-primary flex-1"
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
