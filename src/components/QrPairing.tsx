import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface QrPairingProps {
  onClose: () => void;
}

export function QrPairing({ onClose }: QrPairingProps) {
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("generate_pairing_qr")
      .then(setQrDataUrl)
      .catch((err) => setError(String(err)));
  }, []);

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

        {error ? (
          <div className="rounded-xl border border-destructive/30 bg-destructive/10 p-5">
            <p className="text-sm text-destructive">{error}</p>
          </div>
        ) : qrDataUrl ? (
          <div className="mx-auto inline-block rounded-xl bg-white p-4">
            <img src={qrDataUrl} alt="QR Code" className="size-56" />
          </div>
        ) : (
          <div className="flex items-center justify-center rounded-xl border border-border bg-background/50 p-12">
            <div className="spinner" />
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
