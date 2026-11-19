import { forwardRef, type ReactNode } from "react";

interface EntryCardProps {
  id: string;
  contentType: string;
  lastUsedAt: string;
  isFavorite: boolean;
  sizeBytes: number;
  preview: string | null;
  isSelected: boolean;
  shortcutKey: string | null;
  index: number;
  searchQuery: string;
  onToggleFavorite: (id: string) => void;
  onDelete: (id: string) => void;
  onCopyBack: (id: string) => Promise<void>;
}

type EntryKind = "image" | "link" | "color" | "text";

// Picks the icon shown in front of an entry. Display only.
const entryKind = (contentType: string, preview: string | null): EntryKind => {
  if (contentType === "image") return "image";
  const text = preview?.trim() ?? "";
  if (/^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(text)) return "color";
  if (/^https?:\/\/\S+$/i.test(text)) return "link";
  return "text";
};

export const EntryCard = forwardRef<HTMLDivElement, EntryCardProps>(function EntryCard({
  id,
  contentType,
  lastUsedAt,
  isFavorite,
  sizeBytes,
  preview,
  isSelected,
  shortcutKey,
  searchQuery,
  onToggleFavorite,
  onDelete,
  onCopyBack,
}, ref) {
  const formatTime = (iso: string) => {
    const date = new Date(iso);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);

    if (diffMin < 1) return "now";
    if (diffMin < 60) return `${diffMin}m`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h`;
    const diffDay = Math.floor(diffHr / 24);
    return `${diffDay}d`;
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

  const highlightText = (text: string): ReactNode => {
    const q = searchQuery.trim();
    if (!q) return text;
    const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regex = new RegExp(`(${escaped})`, "gi");
    const parts = text.split(regex);
    if (parts.length === 1) return text;
    return parts.map((part, i) =>
      regex.test(part)
        ? <span key={i} className="rounded-sm bg-primary/25 px-px text-primary">{part}</span>
        : part
    );
  };

  const kind = entryKind(contentType, preview);

  // Hidden until the row is hovered, selected, or holds keyboard focus.
  const revealed = isSelected
    ? "opacity-100"
    : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100";
  const action =
    "rounded p-1 text-muted-foreground transition-[color,background-color,opacity] hover:bg-background/70";

  return (
    <div
      ref={ref}
      onDoubleClick={() => onCopyBack(id)}
      className={`group flex cursor-pointer gap-3 rounded-lg px-3 py-2 transition-colors ${
        isSelected
          ? "bg-secondary shadow-[inset_2px_0_0_var(--color-primary)]"
          : "hover:bg-secondary/50"
      }`}
    >
      <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-background/60 font-mono text-[10px] text-muted-foreground">
        {kind === "text" && "Aa"}
        {kind === "link" && (
          <svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7" />
            <path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7" />
          </svg>
        )}
        {kind === "image" && (
          <svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect width="18" height="18" x="3" y="3" rx="2" />
            <circle cx="9" cy="9" r="2" />
            <path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" />
          </svg>
        )}
        {kind === "color" && (
          <span
            className="size-3 rounded-sm ring-1 ring-foreground/15"
            style={{ backgroundColor: preview?.trim() }}
          />
        )}
      </span>

      <div className="min-w-0 flex-1">
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1 pt-1">
            {contentType === "image" && preview ? (
              <img
                src={preview}
                alt="Clipboard image"
                className="max-h-16 rounded-md border border-border object-contain"
              />
            ) : (
              <p
                className={`line-clamp-3 font-mono text-xs leading-relaxed wrap-anywhere whitespace-pre-wrap ${
                  isSelected ? "text-foreground" : "text-foreground/80"
                }`}
              >
                {contentType === "image" ? null : highlightText(displayContent() ?? "")}
              </p>
            )}
          </div>
          <button
            onClick={() => onToggleFavorite(id)}
            className={`${action} mt-0.5 ${
              isFavorite
                ? "text-primary"
                : `hover:text-foreground ${revealed}`
            }`}
            title={isFavorite ? "Unfavorite" : "Favorite"}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill={isFavorite ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
            </svg>
          </button>
          <span className="shrink-0 pt-1 font-mono text-[11px] leading-5 text-muted-foreground/70">
            {formatTime(lastUsedAt)}
          </span>
        </div>

        <div className="mt-0.5 flex h-5 items-center gap-2 font-mono text-[10px] text-muted-foreground/70">
          {shortcutKey !== null && (
            <span
              className="flex size-4 shrink-0 items-center justify-center rounded-sm border border-border bg-background/60 text-[9px] text-muted-foreground"
              title={`Ctrl+${shortcutKey}`}
            >
              {shortcutKey}
            </span>
          )}
          <span>{formatSize(sizeBytes)}</span>
          <div className="flex-1" />
          <button
            onClick={() => onCopyBack(id)}
            className={`${action} hover:text-foreground ${revealed}`}
            title="Copy & paste"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          </button>
          <button
            onClick={() => onDelete(id)}
            className={`${action} hover:text-destructive ${revealed}`}
            title="Delete"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="3 6 5 6 21 6" />
              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
});
