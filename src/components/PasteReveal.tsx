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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div className="w-full max-w-md overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
        <div className="flex items-center justify-between border-b border-border py-2 pr-2 pl-4">
          <h2 className="text-sm font-semibold text-foreground">Secure paste</h2>
          <button
            type="button"
            onClick={dismiss}
            className="icon-btn"
            aria-label="Close"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="15"
              height="15"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <div className="space-y-3 p-4">
          {loading && (
            <div className="flex items-center justify-center py-8">
              <div className="spinner" />
            </div>
          )}

          {error && !loading && (
            <>
              <p className="text-sm text-foreground/90">{error}</p>
              <p className="text-xs text-muted-foreground">
                The paste may have been opened in the browser already.
              </p>
            </>
          )}

          {plaintext && !loading && (
            <>
              <pre className="max-h-[40vh] overflow-auto rounded-lg border border-border bg-background/70 p-3 font-mono text-xs leading-relaxed break-words whitespace-pre-wrap text-foreground">
                {plaintext}
              </pre>
              <button
                type="button"
                onClick={() => void copyPlaintext()}
                className="btn btn-primary w-full"
              >
                {copied ? "Copied" : "Copy to clipboard"}
              </button>
              <p className="text-xs text-muted-foreground">
                Deleted from the server. This view is one-time only.
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
