import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MnemonicDisplay } from "./MnemonicDisplay";

interface PhraseChangeProps {
  onClose: () => void;
}

type StartResult =
  | { status: "started"; words: string[] }
  | { status: "auth_unavailable" }
  | { status: "cancelled" };

type View =
  | { kind: "intro" }
  | { kind: "confirming" }
  | { kind: "unavailable" }
  | { kind: "cancelled" }
  | { kind: "words"; words: string[] }
  | { kind: "changing" }
  | { kind: "done" }
  | { kind: "error"; message: string; canRetry: boolean };

export function PhraseChange({ onClose }: PhraseChangeProps) {
  const [view, setView] = useState<View>({ kind: "intro" });

  const start = async (allowWithoutOsAuth: boolean) => {
    setView({ kind: "confirming" });
    try {
      const result = await invoke<StartResult>("begin_phrase_change", {
        allowWithoutOsAuth,
      });
      if (result.status === "started") {
        setView({ kind: "words", words: result.words });
      } else if (result.status === "auth_unavailable") {
        setView({ kind: "unavailable" });
      } else {
        setView({ kind: "cancelled" });
      }
    } catch (err) {
      setView({ kind: "error", message: String(err), canRetry: false });
    }
  };

  const finish = async () => {
    setView({ kind: "changing" });
    try {
      await invoke("finish_phrase_change");
      setView({ kind: "done" });
    } catch (err) {
      setView({ kind: "error", message: String(err), canRetry: true });
    }
  };

  const close = async () => {
    if (view.kind !== "done") {
      await invoke("cancel_phrase_change").catch(() => {});
    }
    onClose();
  };

  if (view.kind === "words") {
    return (
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="space-y-3 p-5">
          <MnemonicDisplay words={view.words} onConfirm={finish} />
          <div className="mx-auto w-full max-w-md">
            <button onClick={close} className="btn btn-outline w-full">
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-md space-y-5 p-5">
        <div>
          <h2 className="text-xl font-semibold tracking-tight text-foreground">
            Change your recovery phrase
          </h2>
          <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
            CMDV makes a new 24-word phrase and encrypts your synced history
            again under it. The old phrase stops working. Your other devices ask
            for the new phrase the next time they sync.
          </p>
        </div>

        {view.kind === "intro" && (
          <div className="flex gap-2">
            <button onClick={close} className="btn btn-outline flex-1">
              Cancel
            </button>
            <button
              onClick={() => start(false)}
              className="btn btn-primary flex-1"
            >
              Continue
            </button>
          </div>
        )}

        {(view.kind === "confirming" || view.kind === "changing") && (
          <div className="flex flex-col items-center gap-3 rounded-xl border border-border bg-background/50 p-8">
            <div className="spinner" />
            <p className="text-sm text-muted-foreground">
              {view.kind === "confirming"
                ? "Confirm it's you in the system prompt."
                : "Encrypting your synced history under the new phrase..."}
            </p>
          </div>
        )}

        {view.kind === "unavailable" && (
          <div className="space-y-3">
            <p className="callout-warning">
              This computer can't confirm it's you (Windows Hello isn't set up,
              or your system doesn't offer it). Anyone using this computer can
              change your recovery phrase.
            </p>
            <div className="flex gap-2">
              <button onClick={close} className="btn btn-outline flex-1">
                Cancel
              </button>
              <button
                onClick={() => start(true)}
                className="btn btn-primary flex-1"
              >
                Continue anyway
              </button>
            </div>
          </div>
        )}

        {view.kind === "cancelled" && (
          <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
              Not confirmed, so the phrase stays the same.
            </p>
            <div className="flex gap-2">
              <button onClick={close} className="btn btn-outline flex-1">
                Close
              </button>
              <button
                onClick={() => start(false)}
                className="btn btn-primary flex-1"
              >
                Try again
              </button>
            </div>
          </div>
        )}

        {view.kind === "done" && (
          <div className="space-y-3">
            <p className="text-sm text-foreground">
              Recovery phrase changed. Use the new phrase from now on.
            </p>
            <button onClick={close} className="btn btn-primary w-full">
              Close
            </button>
          </div>
        )}

        {view.kind === "error" && (
          <div className="space-y-3">
            <div className="rounded-xl border border-destructive/30 bg-destructive/10 p-5">
              <p className="text-sm text-destructive">{view.message}</p>
            </div>
            <div className="flex gap-2">
              <button onClick={close} className="btn btn-outline flex-1">
                Cancel
              </button>
              {view.canRetry && (
                <button onClick={finish} className="btn btn-primary flex-1">
                  Try again
                </button>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
