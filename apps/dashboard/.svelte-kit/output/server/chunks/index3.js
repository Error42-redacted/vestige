const NODE_TYPE_COLORS = {
  fact: "#3b82f6",
  // blue
  concept: "#8b5cf6",
  // purple
  event: "#f59e0b",
  // amber
  person: "#10b981",
  // emerald
  place: "#06b6d4",
  // cyan
  note: "#6b7280",
  // gray
  pattern: "#ec4899",
  // pink
  decision: "#ef4444"
  // red
};
const EVENT_TYPE_COLORS = {
  MemoryCreated: "#10b981",
  MemoryUpdated: "#3b82f6",
  MemoryDeleted: "#ef4444",
  SearchPerformed: "#6366f1",
  DreamStarted: "#8b5cf6",
  DreamCompleted: "#a855f7",
  ConsolidationStarted: "#f59e0b",
  ConsolidationCompleted: "#f97316",
  ConnectionDiscovered: "#06b6d4",
  ImportanceScored: "#ec4899",
  Heartbeat: "#6b7280"
};
export {
  EVENT_TYPE_COLORS as E,
  NODE_TYPE_COLORS as N
};
