import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

type PasteRevealPayload = {
  plaintext?: string;
  error?: string;
};

export function PasteReveal() {
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const unsubs: Array<() => void> = [];

    void listen("deep-link-paste-loading", () => {
      setLoading(true);
      setError(null);
      setPlaintext(null);
    }).then((fn) => unsubs.push(fn));

    void listen<PasteRevealPayload>("deep-link-paste", (event) => {
      setLoading(false);
      if (event.payload.error) {
        setError(event.payload.error);
        setPlaintext(null);
        return;
      }
      if (event.payload.plaintext) {
        setPlaintext(event.payload.plaintext);
        setError(null);
      }
    }).then((fn) => unsubs.push(fn));

    return () => {
      for (const fn of unsubs) fn();
    };
  }, []);

  if (!loading && !plaintext && !error) {
    return null;
  }

  const dismiss = () => {
    setPlaintext(null);
    setError(null);
    setLoading(false);
  };

  const copyPlaintext = async () => {
    if (!plaintext) return;
    await navigator.clipboard.writeText(plaintext);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="w-full max-w-md rounded-lg border border-zinc-700 bg-zinc-900 shadow-xl">
        <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
          <h2 className="text-sm font-semibold text-zinc-100">Secure paste</h2>
          <button
            type="button"
            onClick={dismiss}
            className="rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-300"
            aria-label="Close"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <div className="px-4 py-4 space-y-3">
          {loading && (
            <div className="flex items-center justify-center py-8">
              <div className="h-6 w-6 animate-spin rounded-full border-2 border-zinc-600 border-t-zinc-300" />
            </div>
          )}

          {error && !loading && (
            <>
              <p className="text-sm text-zinc-400">{error}</p>
              <p className="text-xs text-zinc-500">
                The paste may have been opened in the browser already.
              </p>
            </>
          )}

          {plaintext && !loading && (
            <>
              <pre className="max-h-[40vh] overflow-auto whitespace-pre-wrap break-words rounded border border-zinc-800 bg-zinc-950 p-3 text-sm text-zinc-200">
                {plaintext}
              </pre>
              <button
                type="button"
                onClick={() => void copyPlaintext()}
                className="w-full rounded-md bg-zinc-100 px-3 py-2 text-sm font-medium text-zinc-900 hover:bg-white"
              >
                {copied ? "Copied" : "Copy to clipboard"}
              </button>
              <p className="text-xs text-zinc-500">
                Deleted from the server. This view is one-time only.
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
