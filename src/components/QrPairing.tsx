interface QrPairingProps {
  qrDataUrl: string | null;
  onScan: () => void;
}

export function QrPairing({ qrDataUrl, onScan: _onScan }: QrPairingProps) {
  return (
    <div className="space-y-6 max-w-md mx-auto p-6 text-center">
      <div>
        <h2 className="text-xl font-bold text-zinc-100">Pair Device</h2>
        <p className="text-zinc-400 text-sm mt-2">
          Scan this QR code from your other device running CMD to transfer your
          encryption key securely.
        </p>
      </div>

      {qrDataUrl ? (
        <div className="bg-white rounded-2xl p-6 inline-block mx-auto">
          <img src={qrDataUrl} alt="QR Code" className="w-48 h-48" />
        </div>
      ) : (
        <div className="bg-zinc-900 rounded-2xl p-12 flex items-center justify-center">
          <p className="text-zinc-500 text-sm">Generating QR code...</p>
        </div>
      )}

      <p className="text-xs text-zinc-500">
        The QR code contains your encrypted recovery phrase. It expires after
        one scan.
      </p>
    </div>
  );
}
