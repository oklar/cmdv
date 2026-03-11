import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EntryCard } from "./EntryCard";

interface Entry {
  id: string;
  content_type: string;
  last_used_at: string;
  is_favorite: boolean;
  is_sensitive: boolean;
  size_bytes: number;
  source_app: string | null;
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
    await invoke("copy_entry_to_clipboard", { id });
    await invoke("hide_to_tray");
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

  return (
    <div className="flex-1 overflow-y-auto">
      {entries.map((entry) => (
        <EntryCard
          key={entry.id}
          id={entry.id}
          contentType={entry.content_type}
          lastUsedAt={entry.last_used_at}
          isFavorite={entry.is_favorite}
          isSensitive={entry.is_sensitive}
          sizeBytes={entry.size_bytes}
          sourceApp={entry.source_app}
          preview={entry.preview}
          onToggleFavorite={handleToggleFavorite}
          onDelete={handleDelete}
          onCopyBack={handleCopyBack}
        />
      ))}
    </div>
  );
}
