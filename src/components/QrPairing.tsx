import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface QrPairingProps {
  onClose: () => void;
}

type RecoveryQr =
  | { status: "shown"; qr: string }
  | { status: "auth_unavailable" }
  | { status: "cancelled" };

type View =
  | { kind: "intro" }
  | { kind: "confirming" }
  | { kind: "unavailable" }
  | { kind: "cancelled" }
  | { kind: "shown"; qr: string }
  | { kind: "error"; message: string };

/** How long the QR code stays on screen. */
const SHOW_SECONDS = 60;

export function QrPairing({ onClose }: QrPairingProps) {
  const [view, setView] = useState<View>({ kind: "intro" });
  const [secondsLeft, setSecondsLeft] = useState(SHOW_SECONDS);

  const request = async (allowWithoutOsAuth: boolean) => {
    setView({ kind: "confirming" });
    try {
      const result = await invoke<RecoveryQr>("show_recovery_qr", {
        allowWithoutOsAuth,
      });
      if (result.status === "shown") {
        setSecondsLeft(SHOW_SECONDS);
        setView({ kind: "shown", qr: result.qr });
      } else if (result.status === "auth_unavailable") {
        setView({ kind: "unavailable" });
      } else {
        setView({ kind: "cancelled" });
      }
    } catch (err) {
      setView({ kind: "error", message: String(err) });
    }
  };

  const hide = useCallback(() => setView({ kind: "intro" }), []);
  const shown = view.kind === "shown";

  // Hide the code after a minute, and as soon as the window is left.
  useEffect(() => {
    if (!shown) return;
    const deadline = Date.now() + SHOW_SECONDS * 1000;
    const timer = setInterval(() => {
      const left = Math.ceil((deadline - Date.now()) / 1000);
      if (left <= 0) hide();
      else setSecondsLeft(left);
    }, 1000);
    const onHidden = () => {
      if (document.visibilityState === "hidden") hide();
    };
    window.addEventListener("blur", hide);
    document.addEventListener("visibilitychange", onHidden);
    return () => {
      clearInterval(timer);
      window.removeEventListener("blur", hide);
      document.removeEventListener("visibilitychange", onHidden);
    };
  }, [shown, hide]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-md space-y-5 p-5 text-center">
        <div>
          <h2 className="text-xl font-semibold tracking-tight text-foreground">
            Pair Device
          </h2>
          <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
            Scan this QR code from your other device running CMDV to transfer
            your recovery phrase securely.
          </p>
        </div>

        {view.kind === "intro" && (
          <button
            onClick={() => request(false)}
            className="btn btn-primary w-full"
          >
            Show QR code
          </button>
        )}

        {view.kind === "confirming" && (
          <div className="flex flex-col items-center gap-3 rounded-xl border border-border bg-background/50 p-8">
            <div className="spinner" />
            <p className="text-sm text-muted-foreground">
              Confirm it's you in the system prompt.
            </p>
          </div>
        )}

        {view.kind === "unavailable" && (
          <div className="space-y-3 text-left">
            <p className="callout-warning">
              This computer can't confirm it's you (Windows Hello isn't set up,
              or your system doesn't offer it). Anyone using this computer can
              open this screen and copy your recovery phrase.
            </p>
            <div className="flex gap-2">
              <button onClick={hide} className="btn btn-outline flex-1">
                Cancel
              </button>
              <button
                onClick={() => request(true)}
                className="btn btn-primary flex-1"
              >
                Show anyway
              </button>
            </div>
          </div>
        )}

        {view.kind === "cancelled" && (
          <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
              Not confirmed, so the QR code stays hidden.
            </p>
            <button
              onClick={() => request(false)}
              className="btn btn-outline w-full"
            >
              Try again
            </button>
          </div>
        )}

        {view.kind === "error" && (
          <div className="rounded-xl border border-destructive/30 bg-destructive/10 p-5">
            <p className="text-sm text-destructive">{view.message}</p>
          </div>
        )}

        {view.kind === "shown" && (
          <div className="space-y-2">
            <div className="mx-auto inline-block rounded-xl bg-white p-4">
              <img src={view.qr} alt="QR Code" className="size-56" />
            </div>
            <div className="flex items-center justify-center gap-3 text-xs text-muted-foreground">
              <span>Hides in {secondsLeft} s</span>
              <button onClick={hide} className="btn btn-sm btn-outline">
                Hide now
              </button>
            </div>
          </div>
        )}

        <p className="callout-warning text-left">
          This QR code contains your 24-word recovery phrase. Only scan it on
          devices you trust. The mnemonic gives full access to your encrypted
          data.
        </p>

        <button onClick={onClose} className="btn btn-outline w-full">
          Done
        </button>
      </div>
    </div>
  );
}
