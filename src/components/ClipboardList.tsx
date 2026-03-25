import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EntryCard } from "./EntryCard";

interface Entry {
  id: string;
  content_type: string;
  last_used_at: string;
  is_favorite: boolean;
  size_bytes: number;
  preview: string | null;
}

interface ClipboardListProps {
  searchQuery: string;
  filterType: string | null;
  favoritesOnly: boolean;
}

export function ClipboardList({
  searchQuery,
  filterType,
  favoritesOnly,
}: ClipboardListProps) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const entryRefs = useRef<(HTMLDivElement | null)[]>([]);
  const didCopyRef = useRef(false);

  const fetchEntries = useCallback(async () => {
    try {
      if (searchQuery.trim()) {
        const results = await invoke<Entry[]>("search_entries", {
          query: searchQuery,
          limit: 50,
        });
        setEntries(results);
      } else {
        const results = await invoke<Entry[]>("get_entries", {
          limit: 50,
          offset: 0,
          contentType: filterType,
          favoritesOnly: favoritesOnly,
        });
        setEntries(results);
      }
    } catch (err) {
      console.error("Failed to fetch entries:", err);
    } finally {
      setLoading(false);
    }
  }, [searchQuery, filterType, favoritesOnly]);

  useEffect(() => {
    fetchEntries();
    const interval = setInterval(fetchEntries, 2000);
    return () => clearInterval(interval);
  }, [fetchEntries]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [searchQuery, filterType, favoritesOnly]);

  useEffect(() => {
    const resetSelection = () => {
      if (didCopyRef.current) {
        setSelectedIndex(0);
        didCopyRef.current = false;
      } else {
        entryRefs.current[selectedIndex]?.scrollIntoView({ block: "nearest" });
      }
    };
    window.addEventListener("focus", resetSelection);
    return () => window.removeEventListener("focus", resetSelection);
  }, [selectedIndex]);

  useEffect(() => {
    entryRefs.current[selectedIndex]?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        invoke("hide_to_tray");
        return;
      }

      if (entries.length === 0) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, entries.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === "PageDown") {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 10, entries.length - 1));
      } else if (e.key === "PageUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 10, 0));
      } else if (e.key === "Home") {
        e.preventDefault();
        setSelectedIndex(0);
      } else if (e.key === "End") {
        e.preventDefault();
        setSelectedIndex(entries.length - 1);
      } else if (e.key === "Enter") {
        e.preventDefault();
        const entry = entries[selectedIndex];
        if (entry) handleCopyBack(entry.id);
      }

      if (e.ctrlKey && !e.altKey && !e.metaKey) {
        const digit = parseInt(e.key, 10);
        if (!isNaN(digit)) {
          const index = digit === 0 ? 9 : digit - 1;
          const entry = entries[index];
          if (entry) {
            e.preventDefault();
            handleCopyBack(entry.id);
          }
        }
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [entries, selectedIndex, searchQuery]);

  const handleToggleFavorite = async (id: string) => {
    try {
      await invoke("toggle_favorite", { id });
      fetchEntries();
    } catch (err) {
      console.error("Failed to toggle favorite:", err);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke("delete_entry", { id });
      fetchEntries();
    } catch (err) {
      console.error("Failed to delete entry:", err);
    }
  };

  const handleCopyBack = async (id: string) => {
    didCopyRef.current = true;
    await invoke("copy_entry_to_clipboard", { id });
    await invoke("hide_to_tray");
    await invoke("simulate_paste");
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-zinc-500 text-sm">Loading...</div>
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center px-8">
        <div className="w-16 h-16 rounded-2xl bg-zinc-900 flex items-center justify-center mb-4">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="28"
            height="28"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            className="text-zinc-600"
          >
            <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
            <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
          </svg>
        </div>
        <p className="text-zinc-400 text-sm text-center">
          {searchQuery
            ? "No results found"
            : "Your clipboard history will appear here"}
        </p>
        <p className="text-zinc-600 text-xs mt-1 text-center">
          Copy something to get started
        </p>
      </div>
    );
  }

  const shortcutKeyForIndex = (index: number): string | null => {
    if (index < 9) return String(index + 1);
    if (index === 9) return "0";
    return null;
  };

  return (
    <div className="flex-1 overflow-y-auto">
      {entries.map((entry, index) => (
        <EntryCard
          key={entry.id}
          ref={(el) => {
            entryRefs.current[index] = el;
          }}
          id={entry.id}
          contentType={entry.content_type}
          lastUsedAt={entry.last_used_at}
          isFavorite={entry.is_favorite}
          sizeBytes={entry.size_bytes}
          preview={entry.preview}
          isSelected={index === selectedIndex}
          shortcutKey={shortcutKeyForIndex(index)}
          index={index}
          searchQuery={searchQuery}
          onToggleFavorite={handleToggleFavorite}
          onDelete={handleDelete}
          onCopyBack={handleCopyBack}
        />
      ))}
    </div>
  );
}
