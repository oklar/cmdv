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

  return (
    <div data-tauri-drag-region className="px-3 py-2 border-b border-zinc-800 space-y-2">
      <div className="relative">
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500"
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          placeholder="Search"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          className="w-full bg-zinc-900 border border-zinc-800 rounded-md pl-10 pr-4 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:ring-1 focus:ring-zinc-600 focus:border-zinc-600 transition-colors"
        />
      </div>
      <div className="flex items-center justify-between">
        <div className="inline-flex rounded-md overflow-hidden border border-zinc-700">
          {(["all", "text", "image"] as const).map((type_) => {
            const isActive =
              (type_ === "all" && !filterType) || filterType === type_;
            return (
              <button
                key={type_}
                onClick={() =>
                  onFilterTypeChange(type_ === "all" ? null : type_)
                }
                className={`px-3 py-1 text-xs transition-colors ${
                  isActive
                    ? "bg-zinc-700 text-zinc-100"
                    : "text-zinc-400 hover:bg-zinc-800 hover:text-zinc-300"
                }`}
              >
                {type_.charAt(0).toUpperCase() + type_.slice(1)}
              </button>
            );
          })}
        </div>

        <button
          onClick={() => onFavoritesOnlyChange(!favoritesOnly)}
          className={`p-1.5 rounded-md transition-colors ${
            favoritesOnly
              ? "text-amber-400"
              : "text-zinc-500 hover:text-zinc-300"
          }`}
          title={favoritesOnly ? "Show all" : "Show favorites only"}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill={favoritesOnly ? "currentColor" : "none"}
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
        </button>
      </div>
    </div>
  );
}
