import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Entry {
  id: string;
  content_type: string;
  created_at: string;
  is_favorite: boolean;
  is_sensitive: boolean;
  size_bytes: number;
  source_app: string | null;
  preview: string | null;
}

interface Stats {
  total_entries: number;
  total_size_bytes: number;
  max_size_bytes: number;
}

export function useClipboard(
  searchQuery: string,
  filterType: string | null,
  favoritesOnly: boolean,
  pollInterval = 2000
) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
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
          favoritesOnly,
        });
        setEntries(results);
      }
    } catch (err) {
      console.error("Failed to fetch entries:", err);
    } finally {
      setLoading(false);
    }
  }, [searchQuery, filterType, favoritesOnly]);

  const fetchStats = useCallback(async () => {
    try {
      const s = await invoke<Stats>("get_stats");
      setStats(s);
    } catch (err) {
      console.error("Failed to fetch stats:", err);
    }
  }, []);

  useEffect(() => {
    fetchEntries();
    fetchStats();
    const interval = setInterval(() => {
      fetchEntries();
      fetchStats();
    }, pollInterval);
    return () => clearInterval(interval);
  }, [fetchEntries, fetchStats, pollInterval]);

  const toggleFavorite = async (id: string) => {
    await invoke("toggle_favorite", { id });
    fetchEntries();
  };

  const deleteEntry = async (id: string) => {
    await invoke("delete_entry", { id });
    fetchEntries();
    fetchStats();
  };

  return { entries, stats, loading, toggleFavorite, deleteEntry, refresh: fetchEntries };
}
