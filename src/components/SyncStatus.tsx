export function SyncStatus() {
  // Sync UI will be connected in Phase 4
  return (
    <div className="flex items-center gap-1.5 text-xs text-zinc-500">
      <div className="w-2 h-2 rounded-full bg-emerald-500" />
      <span>Local</span>
    </div>
  );
}
