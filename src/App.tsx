import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { ClipboardList } from "./components/ClipboardList";
import { SearchBar } from "./components/SearchBar";
import { Settings } from "./components/Settings";
import { SetupWizard } from "./components/SetupWizard";
import { AppLock } from "./components/AppLock";
import { PasteReveal } from "./components/PasteReveal";
import { TitleBar } from "./components/TitleBar";

type AppState = "loading" | "setup" | "locked" | "unlocked";
type View = "clipboard" | "settings";

export default function App() {
  const [appState, setAppState] = useState<AppState>("loading");
  const [view, setView] = useState<View>("clipboard");
  const [searchQuery, setSearchQuery] = useState("");
  const [filterType, setFilterType] = useState<string | null>(null);
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [version, setVersion] = useState("");
  const autostartTrayAppliedRef = useRef(false);

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

  useEffect(() => {
    if (appState !== "unlocked") return;
    if (autostartTrayAppliedRef.current) return;
    autostartTrayAppliedRef.current = true;
    invoke("apply_autostart_tray").catch(() => {});
  }, [appState]);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  let content: ReactNode;

  if (appState === "loading") {
    content = (
      <div className="flex min-h-screen items-center justify-center bg-card">
        <div className="spinner" />
      </div>
    );
  } else if (appState === "setup") {
    content = (
      <SetupWizard
        onComplete={() => {
          invoke("finish_setup");
          setAppState("unlocked");
        }}
      />
    );
  } else if (appState === "locked") {
    content = <AppLock onUnlock={() => setAppState("unlocked")} />;
  } else {
    content = (
      <div className="flex h-screen flex-col overflow-hidden bg-card text-foreground">
        <TitleBar version={version}>
          <button
            onClick={() =>
              setView(view === "settings" ? "clipboard" : "settings")
            }
            className="icon-btn"
            title={view === "settings" ? "Back" : "Settings"}
          >
            <svg
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
              {view === "settings" ? (
                <path d="M19 12H5M12 19l-7-7 7-7" />
              ) : (
                <>
                  <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
                  <circle cx="12" cy="12" r="3" />
                </>
              )}
            </svg>
          </button>
        </TitleBar>

        <div className="flex min-h-0 flex-1 flex-col">
          {view === "clipboard" ? (
            <>
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
                onFilterTypeChange={setFilterType}
                onFavoritesOnlyChange={setFavoritesOnly}
              />
            </>
          ) : (
            <Settings />
          )}
        </div>
      </div>
    );
  }

  return (
    <>
      <PasteReveal />
      {content}
    </>
  );
}
