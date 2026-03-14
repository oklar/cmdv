import { forwardRef, useState } from "react";

interface EntryCardProps {
  id: string;
  contentType: string;
  lastUsedAt: string;
  isFavorite: boolean;
  isSensitive: boolean;
  sizeBytes: number;
  sourceApp: string | null;
  preview: string | null;
  isSelected: boolean;
  shortcutKey: string | null;
  onToggleFavorite: (id: string) => void;
  onDelete: (id: string) => void;
  onCopyBack: (id: string) => Promise<void>;
}

export const EntryCard = forwardRef<HTMLDivElement, EntryCardProps>(function EntryCard({
  id,
  contentType,
  lastUsedAt,
  isFavorite,
  isSensitive,
  sizeBytes,
  sourceApp,
  preview,
  isSelected,
  shortcutKey,
  onToggleFavorite,
  onDelete,
  onCopyBack,
}, ref) {
  const [revealed, setRevealed] = useState(false);

  const formatTime = (iso: string) => {
    const date = new Date(iso);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);

    if (diffMin < 1) return "Just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDay = Math.floor(diffHr / 24);
    return `${diffDay}d ago`;
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const displayContent = () => {
    if (!preview) return "[No preview]";
    if (contentType === "image") return null;
    if (isSensitive && !revealed) return "••••••••••••";
    return preview;
  };

  return (
    <div
      ref={ref}
      className={`group px-4 py-3 border-b border-zinc-800/50 transition-colors ${
        isSelected ? "bg-zinc-800/70 ring-1 ring-zinc-600" : "hover:bg-zinc-900/50"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        {shortcutKey !== null && (
          <span className="shrink-0 w-4 h-4 rounded-sm bg-zinc-800/80 text-zinc-500 text-[10px] font-medium flex items-center justify-center mt-0.5">
            {shortcutKey}
          </span>
        )}
        <div className="flex-1 min-w-0">
          {contentType === "image" && preview ? (
            <img
              src={preview}
              alt="Clipboard image"
              className="max-h-24 rounded border border-zinc-700 object-contain"
            />
          ) : (
            <p className="text-sm text-zinc-200 truncate font-mono">
              {displayContent()}
            </p>
          )}
          <div className="flex items-center gap-2 mt-1.5">
            <span className="text-xs text-zinc-500">{formatTime(lastUsedAt)}</span>
            <span className="text-xs text-zinc-600">·</span>
            <span className="text-xs text-zinc-500">{formatSize(sizeBytes)}</span>
            {contentType === "image" && (
              <>
                <span className="text-xs text-zinc-600">·</span>
                <span className="text-xs text-blue-400">Image</span>
              </>
            )}
            {isSensitive && (
              <>
                <span className="text-xs text-zinc-600">·</span>
                <span className="text-xs text-red-400">Sensitive</span>
              </>
            )}
            {sourceApp && (
              <>
                <span className="text-xs text-zinc-600">·</span>
                <span className="text-xs text-zinc-500">{sourceApp}</span>
              </>
            )}
          </div>
        </div>

        <div className={`flex items-center gap-1 transition-opacity ${
          isSelected ? "opacity-100" : "opacity-0 group-hover:opacity-100"
        }`}>
          <button
            onClick={() => onCopyBack(id)}
            className="p-1.5 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
            title="Copy to clipboard"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          </button>

          {isSensitive && (
            <button
              onClick={() => setRevealed(!revealed)}
              className="p-1.5 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
              title={revealed ? "Hide" : "Reveal"}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                {revealed ? (
                  <>
                    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
                    <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19" />
                    <line x1="1" y1="1" x2="23" y2="23" />
                  </>
                ) : (
                  <>
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                    <circle cx="12" cy="12" r="3" />
                  </>
                )}
              </svg>
            </button>
          )}

          <button
            onClick={() => onToggleFavorite(id)}
            className={`p-1.5 rounded hover:bg-zinc-800 transition-colors ${
              isFavorite
                ? "text-amber-400"
                : "text-zinc-500 hover:text-zinc-300"
            }`}
            title={isFavorite ? "Unfavorite" : "Favorite"}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill={isFavorite ? "currentColor" : "none"}
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
            </svg>
          </button>

          <button
            onClick={() => onDelete(id)}
            className="p-1.5 rounded hover:bg-zinc-800 text-zinc-500 hover:text-red-400 transition-colors"
            title="Delete"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <polyline points="3 6 5 6 21 6" />
              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
});
