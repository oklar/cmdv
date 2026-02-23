import { useState } from "react";

interface MnemonicDisplayProps {
  words: string[];
  onConfirm: () => void;
}

export function MnemonicDisplay({ words, onConfirm }: MnemonicDisplayProps) {
  const [confirmed, setConfirmed] = useState(false);

  return (
    <div className="space-y-6 max-w-md mx-auto p-6">
      <div>
        <h2 className="text-xl font-bold text-zinc-100">
          Your Recovery Phrase
        </h2>
        <p className="text-zinc-400 text-sm mt-2">
          Write these 24 words down and store them somewhere safe. You will need
          them to set up CMD on a new device. This is the only time they will be
          shown.
        </p>
      </div>

      <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
        <div className="grid grid-cols-3 gap-2">
          {words.map((word, i) => (
            <div key={i} className="flex items-center gap-2 py-1">
              <span className="text-xs text-zinc-600 w-5 text-right">
                {i + 1}.
              </span>
              <span className="text-sm font-mono text-zinc-200">{word}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="bg-amber-500/10 border border-amber-500/20 rounded-lg p-4">
        <p className="text-amber-400 text-xs">
          If you lose these words and all your devices, your synced data cannot
          be recovered. CMD support cannot help — this is by design for your
          security.
        </p>
      </div>

      <label className="flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(e) => setConfirmed(e.target.checked)}
          className="mt-0.5 rounded border-zinc-600 bg-zinc-800 text-blue-500 focus:ring-blue-500"
        />
        <span className="text-sm text-zinc-400">
          I have written down my recovery phrase and stored it safely.
        </span>
      </label>

      <button
        onClick={onConfirm}
        disabled={!confirmed}
        className="w-full py-3 bg-blue-600 hover:bg-blue-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-white font-medium rounded-lg transition-colors"
      >
        Continue
      </button>
    </div>
  );
}
