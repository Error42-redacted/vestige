import { writable, derived } from 'svelte/store';
import type { VestigeEvent } from '$types';

const MAX_EVENTS = 200;

function createWebSocketStore() {
	const { subscribe, set, update } = writable<{
		connected: boolean;
		events: VestigeEvent[];
		lastHeartbeat: VestigeEvent | null;
		error: string | null;
	}>({
		connected: false,
		events: [],
		lastHeartbeat: null,
		error: null
	});

	let ws: WebSocket | null = null;
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	let reconnectAttempts = 0;

	function connect(url?: string) {
		const wsUrl = url || (window.location.port === '5173'
			? `ws://${window.location.hostname}:3927/ws`
			: `ws://${window.location.host}/ws`);

		if (ws?.readyState === WebSocket.OPEN) return;

		try {
			ws = new WebSocket(wsUrl);

			ws.onopen = () => {
				reconnectAttempts = 0;
				update(s => ({ ...s, connected: true, error: null }));
			};

			ws.onmessage = (event) => {
				try {
					const parsed: VestigeEvent = JSON.parse(event.data);
					update(s => {
						if (parsed.type === 'Heartbeat') {
							return { ...s, lastHeartbeat: parsed };
						}
						const events = [parsed, ...s.events].slice(0, MAX_EVENTS);
						return { ...s, events };
					});
				} catch {
					// Ignore malformed messages
				}
			};

			ws.onclose = () => {
				update(s => ({ ...s, connected: false }));
				scheduleReconnect(wsUrl);
			};

			ws.onerror = () => {
				update(s => ({ ...s, error: 'WebSocket connection failed' }));
			};
		} catch (e) {
			update(s => ({ ...s, error: String(e) }));
		}
	}

	function scheduleReconnect(url: string) {
		if (reconnectTimer) clearTimeout(reconnectTimer);
		const delay = Math.min(1000 * 2 ** reconnectAttempts, 30000);
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
		update(s => ({ ...s, events: [] }));
	}

	return {
		subscribe,
		connect,
		disconnect,
		clearEvents
	};
}

export const websocket = createWebSocketStore();

// Derived stores for specific event types
export const isConnected = derived(websocket, $ws => $ws.connected);
export const eventFeed = derived(websocket, $ws => $ws.events);
export const heartbeat = derived(websocket, $ws => $ws.lastHeartbeat);
export const memoryCount = derived(websocket, $ws =>
	($ws.lastHeartbeat?.data?.memory_count as number) ?? 0
);
export const avgRetention = derived(websocket, $ws =>
	($ws.lastHeartbeat?.data?.avg_retention as number) ?? 0
);
