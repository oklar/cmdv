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
    const onFocus = () => inputRef.current?.focus();
    window.addEventListener("focus", onFocus);
    onFocus();
    return () => window.removeEventListener("focus", onFocus);
  }, []);
  return (
    <div className="px-4 py-3 border-b border-zinc-800 space-y-2">
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
          placeholder="Search clipboard history..."
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          className="w-full bg-zinc-900 border border-zinc-800 rounded-lg pl-10 pr-4 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:ring-1 focus:ring-zinc-600 focus:border-zinc-600 transition-colors"
        />
      </div>
      <div className="flex gap-2">
        {(["all", "text", "image"] as const).map((type_) => (
          <button
            key={type_}
            onClick={() =>
              onFilterTypeChange(type_ === "all" ? null : type_)
            }
            className={`px-3 py-1 text-xs rounded-full transition-colors ${
              (type_ === "all" && !filterType) || filterType === type_
                ? "bg-zinc-100 text-zinc-900"
                : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
            }`}
          >
            {type_.charAt(0).toUpperCase() + type_.slice(1)}
          </button>
        ))}
        <button
          onClick={() => onFavoritesOnlyChange(!favoritesOnly)}
          className={`px-3 py-1 text-xs rounded-full transition-colors ${
            favoritesOnly
              ? "bg-amber-500/20 text-amber-400"
              : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
          }`}
        >
          Favorites
        </button>
      </div>
    </div>
  );
}
