import { d as derived, w as writable } from "./index2.js";
const MAX_EVENTS = 200;
function createWebSocketStore() {
  const { subscribe, set, update } = writable({
    connected: false,
    events: [],
    lastHeartbeat: null,
    error: null
  });
  let ws = null;
  let reconnectTimer = null;
  let reconnectAttempts = 0;
  function connect(url) {
    const wsUrl = url || (window.location.port === "5173" ? `ws://${window.location.hostname}:3927/ws` : `ws://${window.location.host}/ws`);
    if (ws?.readyState === WebSocket.OPEN) return;
    try {
      ws = new WebSocket(wsUrl);
      ws.onopen = () => {
        reconnectAttempts = 0;
        update((s) => ({ ...s, connected: true, error: null }));
      };
      ws.onmessage = (event) => {
        try {
          const parsed = JSON.parse(event.data);
          update((s) => {
            if (parsed.type === "Heartbeat") {
              return { ...s, lastHeartbeat: parsed };
            }
            const events = [parsed, ...s.events].slice(0, MAX_EVENTS);
            return { ...s, events };
          });
        } catch {
        }
      };
      ws.onclose = () => {
        update((s) => ({ ...s, connected: false }));
        scheduleReconnect(wsUrl);
      };
      ws.onerror = () => {
        update((s) => ({ ...s, error: "WebSocket connection failed" }));
      };
    } catch (e) {
      update((s) => ({ ...s, error: String(e) }));
    }
  }
  function scheduleReconnect(url) {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    const delay = Math.min(1e3 * 2 ** reconnectAttempts, 3e4);
    reconnectAttempts++;
    reconnectTimer = setTimeout(() => connect(url), delay);
  }
  function disconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
    ws = null;
    set({ connected: false, events: [], lastHeartbeat: null, error: null });
  }
  function clearEvents() {
    update((s) => ({ ...s, events: [] }));
  }
  return {
    subscribe,
    connect,
    disconnect,
    clearEvents
  };
}
const websocket = createWebSocketStore();
const isConnected = derived(websocket, ($ws) => $ws.connected);
const eventFeed = derived(websocket, ($ws) => $ws.events);
derived(websocket, ($ws) => $ws.lastHeartbeat);
const memoryCount = derived(
  websocket,
  ($ws) => $ws.lastHeartbeat?.data?.memory_count ?? 0
);
const avgRetention = derived(
  websocket,
  ($ws) => $ws.lastHeartbeat?.data?.avg_retention ?? 0
);
export {
  avgRetention as a,
  eventFeed as e,
  isConnected as i,
  memoryCount as m
};
