import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import appIcon from "../assets/icon.png";

interface TitleBarProps {
  version?: string;
  children?: ReactNode;
}

/** Draggable window header shared by every screen. Children render as extra
    actions to the left of the hide button. */
export function TitleBar({ version, children }: TitleBarProps) {
  return (
    <header
      data-tauri-drag-region
      className="flex h-11 shrink-0 items-center gap-2.5 border-b border-border pl-3.5 pr-2 select-none"
    >
      <img src={appIcon} alt="" className="pointer-events-none size-5" />
      <span className="pointer-events-none font-mono text-xs font-medium tracking-[0.14em] text-muted-foreground">
        CMDV
      </span>
      {version && (
        <span className="pointer-events-none font-mono text-[11px] text-muted-foreground/60">
          v{version}
        </span>
      )}
      <div className="ml-auto flex items-center gap-0.5">
        {children}
        <button
          onClick={() => invoke("hide_to_tray")}
          className="icon-btn"
          title="Hide to tray"
        >
          <svg
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
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </header>
  );
}
