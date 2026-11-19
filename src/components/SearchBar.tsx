import { useEffect, useRef } from "react";

interface SearchBarProps {
  query: string;
  onQueryChange: (query: string) => void;
  filterType: string | null;
  onFilterTypeChange: (type_: string | null) => void;
  favoritesOnly: boolean;
  onFavoritesOnlyChange: (fav: boolean) => void;
}

export function SearchBar({
  query,
  onQueryChange,
  filterType,
  onFilterTypeChange,
  favoritesOnly,
  onFavoritesOnlyChange,
}: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onFocus = () => inputRef.current?.focus({ preventScroll: true });
    window.addEventListener("focus", onFocus);
    onFocus();
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const filters = [
    { type: "all", label: "All" },
    { type: "text", label: "Text" },
    { type: "image", label: "Images" },
  ] as const;

  const chip = (active: boolean) =>
    `rounded-md px-2 py-1 transition-colors ${
      active
        ? "bg-primary/15 text-primary"
        : "text-muted-foreground hover:bg-secondary hover:text-foreground"
    }`;

  return (
    <div data-tauri-drag-region className="px-3 pt-3 pb-1">
      <label className="flex cursor-text items-center gap-2.5 rounded-lg border border-border bg-background/70 px-3 py-2">
        <svg
          className="shrink-0 text-muted-foreground"
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
          <circle cx="11" cy="11" r="7" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          placeholder="Search history"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          className="min-w-0 flex-1 bg-transparent text-sm text-foreground caret-primary outline-none placeholder:text-muted-foreground"
        />
      </label>
      <div className="mt-2.5 flex items-center gap-1.5 font-mono text-[11px]">
        {filters.map(({ type, label }) => {
          const isActive =
            (type === "all" && !filterType) || filterType === type;
          return (
            <button
              key={type}
              onClick={() => onFilterTypeChange(type === "all" ? null : type)}
              className={chip(isActive)}
            >
              {label}
            </button>
          );
        })}
        <button
          onClick={() => onFavoritesOnlyChange(!favoritesOnly)}
          aria-pressed={favoritesOnly}
          className={`ml-auto inline-flex items-center gap-1.5 ${chip(favoritesOnly)}`}
          title={favoritesOnly ? "Show all" : "Show favorites only"}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="11"
            height="11"
            viewBox="0 0 24 24"
            fill={favoritesOnly ? "currentColor" : "none"}
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
          Favorites
        </button>
      </div>
    </div>
  );
}
