import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ClipboardList } from "./components/ClipboardList";
import { SearchBar } from "./components/SearchBar";
import { SyncStatus } from "./components/SyncStatus";
import { Settings } from "./components/Settings";
import { SetupWizard } from "./components/SetupWizard";
import { AppLock } from "./components/AppLock";

type AppState = "loading" | "setup" | "locked" | "unlocked";
type View = "clipboard" | "settings";

export default function App() {
  const [appState, setAppState] = useState<AppState>("loading");
  const [view, setView] = useState<View>("clipboard");
  const [searchQuery, setSearchQuery] = useState("");
  const [filterType, setFilterType] = useState<string | null>(null);
  const [favoritesOnly, setFavoritesOnly] = useState(false);

  useEffect(() => {
    invoke<{ setup_complete: boolean; locked: boolean }>("get_vault_status")
      .then(async (status) => {
        if (!status.setup_complete) {
          setAppState("setup");
        } else if (status.locked) {
          const unlocked = await invoke<boolean>("try_auto_unlock");
          setAppState(unlocked ? "unlocked" : "locked");
        } else {
          setAppState("unlocked");
        }
      })
      .catch(() => setAppState("setup"));
  }, []);

  if (appState === "loading") {
    return (
      <div className="min-h-screen bg-zinc-950 flex items-center justify-center">
        <div className="w-6 h-6 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
      </div>
    );
  }

  if (appState === "setup") {
    return <SetupWizard onComplete={() => setAppState("unlocked")} />;
  }

  if (appState === "locked") {
    return <AppLock onUnlock={() => setAppState("unlocked")} />;
  }

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col">
      <div
        data-tauri-drag-region
        className="flex items-center justify-between px-4 py-3 border-b border-zinc-800 bg-zinc-900/80 backdrop-blur-sm sticky top-0 z-10"
      >
        <h1 className="text-lg font-semibold tracking-tight select-none pointer-events-none">CMDV</h1>
        <div className="flex items-center gap-1">
          <SyncStatus />
          <button
            onClick={() => setView(view === "settings" ? "clipboard" : "settings")}
            className="p-2 rounded-lg hover:bg-zinc-800 transition-colors"
            title="Settings"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              {view === "settings" ? (
                <path d="M19 12H5M12 19l-7-7 7-7" />
              ) : (
                <>
                  <circle cx="12" cy="12" r="3" />
                  <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
                </>
              )}
            </svg>
          </button>
          <button
            onClick={async () => {
              const { getCurrentWindow } = await import("@tauri-apps/api/window");
              getCurrentWindow().hide();
            }}
            className="p-2 rounded-lg hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
            title="Hide to tray"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      {view === "clipboard" ? (
        <main className="flex-1 flex flex-col">
          <SearchBar
            query={searchQuery}
            onQueryChange={setSearchQuery}
            filterType={filterType}
            onFilterTypeChange={setFilterType}
            favoritesOnly={favoritesOnly}
            onFavoritesOnlyChange={setFavoritesOnly}
          />
          <ClipboardList
            searchQuery={searchQuery}
            filterType={filterType}
            favoritesOnly={favoritesOnly}
          />
        </main>
      ) : (
        <Settings />
      )}
    </div>
  );
}
