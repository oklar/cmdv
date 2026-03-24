import { forwardRef } from "react";

interface EntryCardProps {
  id: string;
  contentType: string;
  lastUsedAt: string;
  isFavorite: boolean;
  sizeBytes: number;
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
  sizeBytes,
  preview,
  isSelected,
  shortcutKey,
  onToggleFavorite,
  onDelete,
  onCopyBack,
}, ref) {
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
    return preview;
  };

  return (
    <div
      ref={ref}
      onDoubleClick={() => onCopyBack(id)}
      className={`group px-2.5 py-1.5 border-b border-zinc-800/50 transition-colors cursor-pointer ${
        isSelected ? "bg-zinc-800/70 ring-1 ring-lime-500/40" : "hover:bg-zinc-900/50"
      }`}
    >
      {contentType === "image" && preview ? (
        <img
          src={preview}
          alt="Clipboard image"
          className="max-h-16 rounded border border-zinc-700 object-contain"
        />
      ) : (
        <p className="text-xs text-zinc-300 font-mono leading-relaxed line-clamp-3 whitespace-pre-wrap break-all">
          {displayContent()}
        </p>
      )}

      <div className="flex items-center gap-1.5 mt-1">
        {shortcutKey !== null && (
          <span className="w-4 h-4 rounded-sm bg-zinc-800/80 text-zinc-500 text-[9px] font-medium flex items-center justify-center shrink-0">
            {shortcutKey}
          </span>
        )}
        <button
          onClick={() => onToggleFavorite(id)}
          className={`p-0.5 rounded hover:bg-zinc-800 transition-colors ${
            isFavorite ? "text-amber-400" : "text-zinc-600 hover:text-zinc-300"
          }`}
          title={isFavorite ? "Unfavorite" : "Favorite"}
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill={isFavorite ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
        </button>
        <button
          onClick={() => onCopyBack(id)}
          className="p-0.5 rounded hover:bg-zinc-800 text-zinc-600 hover:text-zinc-300 transition-colors"
          title="Copy & paste"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        </button>
        <span className="text-[10px] text-zinc-500">{formatTime(lastUsedAt)}</span>
        <span className="text-[10px] text-zinc-600">{formatSize(sizeBytes)}</span>
        {contentType === "image" && (
          <span className="text-[10px] text-lime-400">Image</span>
        )}
        <div className="flex-1" />
        <button
          onClick={() => onDelete(id)}
          className="p-0.5 rounded hover:bg-zinc-800 text-zinc-600 hover:text-red-400 transition-colors"
          title="Delete"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="3 6 5 6 21 6" />
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
          </svg>
        </button>
      </div>
    </div>
  );
});
