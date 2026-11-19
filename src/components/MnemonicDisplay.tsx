import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

interface MnemonicDisplayProps {
  words: string[];
  onConfirm: () => void;
}

export function MnemonicDisplay({ words, onConfirm }: MnemonicDisplayProps) {
  const [confirmed, setConfirmed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [saving, setSaving] = useState(false);

  const handleSave = async (format: "pdf" | "txt") => {
    const ext = format === "pdf" ? "pdf" : "txt";
    const defaultName =
      format === "pdf" ? "cmdv-recovery-kit.pdf" : "cmdv-recovery-phrase.txt";

    const path = await save({
      defaultPath: defaultName,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });

    if (!path) return;

    setSaving(true);
    try {
      await invoke("export_mnemonic", { path, words, format });
    } catch (e) {
      console.error("Export failed:", e);
    } finally {
      setSaving(false);
    }
  };

  const handleCopy = async () => {
    await navigator.clipboard.writeText(words.join(" "));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="mx-auto w-full max-w-md space-y-3">
      <div>
        <h2 className="text-xl font-semibold tracking-tight text-foreground">
          Your Recovery Phrase
        </h2>
        <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
          Write these 24 words down and store them somewhere safe. You will need
          them to set up CMDV on a new device. This is the only time they will
          be shown.
        </p>
      </div>

      <div className="grid grid-cols-3 gap-1 font-mono">
        {words.map((word, i) => (
          <div
            key={i}
            className="flex items-center gap-1.5 rounded-md border border-border bg-background/60 px-2 py-[3px]"
          >
            <span className="w-4 shrink-0 text-right text-[10px] text-muted-foreground/60">
              {i + 1}
            </span>
            <span className="truncate text-xs text-foreground">{word}</span>
          </div>
        ))}
      </div>

      <div className="flex gap-2">
        <button
          onClick={() => handleSave("pdf")}
          disabled={saving}
          className="btn btn-outline h-8 flex-1 px-2 text-xs"
        >
          Save as PDF
        </button>
        <button
          onClick={() => handleSave("txt")}
          disabled={saving}
          className="btn btn-outline h-8 flex-1 px-2 text-xs"
        >
          Save as Text
        </button>
        <button
          onClick={handleCopy}
          className="btn btn-outline h-8 flex-1 px-2 text-xs"
        >
          {copied ? "Copied!" : "Copy"}
        </button>
      </div>

      <p className="callout-warning">
        If you lose these words and all your devices, your data cannot be
        recovered. CMDV support cannot help. This is by design, for your
        security.
      </p>

      <label className="flex cursor-pointer items-center gap-2.5">
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(e) => setConfirmed(e.target.checked)}
          className="size-4 shrink-0"
        />
        <span className="text-xs leading-4 text-muted-foreground">
          I have written down my recovery phrase and stored it safely.
        </span>
      </label>

      <button
        onClick={onConfirm}
        disabled={!confirmed}
        className="btn btn-primary w-full"
      >
        Continue
      </button>
    </div>
  );
}
