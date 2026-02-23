import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MnemonicDisplay } from "./MnemonicDisplay";

interface SetupWizardProps {
  onComplete: () => void;
}

type Step = "welcome" | "password" | "mnemonic";

export function SetupWizard({ onComplete }: SetupWizardProps) {
  const [step, setStep] = useState<Step>("welcome");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [mnemonicWords, setMnemonicWords] = useState<string[]>([]);

  const handleCreateVault = async () => {
    if (password.length < 8) {
      setError("Password must be at least 8 characters");
      return;
    }
    if (password !== confirm) {
      setError("Passwords do not match");
      return;
    }

    setError("");
    setLoading(true);
    try {
      const result = await invoke<{ mnemonic: string[] }>("setup_vault", {
        password,
      });
      setMnemonicWords(result.mnemonic);
      setStep("mnemonic");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  if (step === "mnemonic") {
    return (
      <div className="min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-8">
        <MnemonicDisplay words={mnemonicWords} onConfirm={onComplete} />
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-8">
      <div className="max-w-md w-full space-y-8">
        {step === "welcome" && (
          <div className="text-center space-y-6">
            <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-blue-500 to-blue-600 flex items-center justify-center mx-auto">
              <span className="text-3xl font-bold text-white">C</span>
            </div>
            <div>
              <h1 className="text-2xl font-bold">Welcome to CMD</h1>
              <p className="text-zinc-400 mt-2 text-sm">
                Your encrypted clipboard manager. Everything is encrypted
                locally with a key only you control.
              </p>
            </div>
            <button
              onClick={() => setStep("password")}
              className="w-full py-3 bg-blue-600 hover:bg-blue-500 text-white font-medium rounded-lg transition-colors"
            >
              Get started
            </button>
          </div>
        )}

        {step === "password" && (
          <div className="space-y-6">
            <div>
              <h2 className="text-xl font-bold">Create your vault password</h2>
              <p className="text-zinc-400 text-sm mt-1">
                This password protects your encryption key. You'll need it every
                time you open CMD. Choose something strong.
              </p>
            </div>

            <div className="space-y-3">
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Password (min 8 characters)"
                autoFocus
                className="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-3 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
              />
              <input
                type="password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                placeholder="Confirm password"
                className="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-3 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
              />
            </div>

            {error && (
              <p className="text-red-400 text-xs text-center">{error}</p>
            )}

            <div className="flex gap-3">
              <button
                onClick={() => setStep("welcome")}
                className="flex-1 py-3 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-medium rounded-lg transition-colors"
              >
                Back
              </button>
              <button
                onClick={handleCreateVault}
                disabled={loading}
                className="flex-1 py-3 bg-blue-600 hover:bg-blue-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-white font-medium rounded-lg transition-colors"
              >
                {loading ? "Creating vault..." : "Create vault"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
