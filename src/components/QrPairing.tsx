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
    <div className="space-y-6 max-w-md mx-auto p-6 text-center">
      <div>
        <h2 className="text-xl font-bold text-zinc-100">Pair Device</h2>
        <p className="text-zinc-400 text-sm mt-2">
          Scan this QR code from your other device running CMD to transfer your
          recovery phrase securely.
        </p>
      </div>

      {error ? (
        <div className="bg-red-900/30 rounded-2xl p-6">
          <p className="text-red-400 text-sm">{error}</p>
        </div>
      ) : qrDataUrl ? (
        <div className="bg-white rounded-2xl p-6 inline-block mx-auto">
          <img src={qrDataUrl} alt="QR Code" className="w-64 h-64" />
        </div>
      ) : (
        <div className="bg-zinc-900 rounded-2xl p-12 flex items-center justify-center">
          <div className="w-6 h-6 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
        </div>
      )}

      <div className="bg-amber-900/30 border border-amber-700/50 rounded-lg p-4">
        <p className="text-xs text-amber-300">
          This QR code contains your 24-word recovery phrase. Only scan it on
          devices you trust. The mnemonic gives full access to your encrypted data.
        </p>
      </div>

      <button
        onClick={onClose}
        className="w-full py-2 px-4 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm rounded-lg transition-colors"
      >
        Done
      </button>
    </div>
  );
}
