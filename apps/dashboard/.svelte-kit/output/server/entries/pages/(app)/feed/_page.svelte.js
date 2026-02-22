import { c as escape_html, s as store_get, e as ensure_array_like, ac as attr_style, d as stringify, f as unsubscribe_stores } from "../../../../chunks/index.js";
import { e as eventFeed } from "../../../../chunks/websocket.js";
import { E as EVENT_TYPE_COLORS } from "../../../../chunks/index3.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    var $$store_subs;
    function formatTime(ts) {
      return new Date(ts).toLocaleTimeString();
    }
    function eventIcon(type) {
      const icons = {
        MemoryCreated: "+",
        MemoryUpdated: "~",
        MemoryDeleted: "×",
        MemoryPromoted: "↑",
        MemoryDemoted: "↓",
        SearchPerformed: "◎",
        DreamStarted: "◈",
        DreamProgress: "◈",
        DreamCompleted: "◈",
        ConsolidationStarted: "◉",
        ConsolidationCompleted: "◉",
        RetentionDecayed: "↘",
        ConnectionDiscovered: "━",
        ActivationSpread: "◬",
        ImportanceScored: "◫",
        Heartbeat: "♡"
      };
      return icons[type] || "·";
    }
    function eventSummary(event) {
      const d = event.data;
      switch (event.type) {
        case "MemoryCreated":
          return `New ${d.node_type}: "${String(d.content_preview).slice(0, 60)}..."`;
        case "SearchPerformed":
          return `Searched "${d.query}" → ${d.result_count} results (${d.duration_ms}ms)`;
        case "DreamStarted":
          return `Dream started with ${d.memory_count} memories`;
        case "DreamCompleted":
          return `Dream complete: ${d.connections_found} connections, ${d.insights_generated} insights (${d.duration_ms}ms)`;
        case "ConsolidationStarted":
          return "Consolidation cycle started";
        case "ConsolidationCompleted":
          return `Consolidated ${d.nodes_processed} nodes, ${d.decay_applied} decayed (${d.duration_ms}ms)`;
        case "ConnectionDiscovered":
          return `Connection: ${String(d.connection_type)} (weight: ${Number(d.weight).toFixed(2)})`;
        case "ImportanceScored":
          return `Scored ${Number(d.composite_score).toFixed(2)}: "${String(d.content_preview).slice(0, 50)}..."`;
        case "MemoryPromoted":
          return `Promoted → ${(Number(d.new_retention) * 100).toFixed(0)}% retention`;
        case "MemoryDemoted":
          return `Demoted → ${(Number(d.new_retention) * 100).toFixed(0)}% retention`;
        default:
          return JSON.stringify(d).slice(0, 100);
      }
    }
    $$renderer2.push(`<div class="p-6 max-w-4xl mx-auto space-y-6"><div class="flex items-center justify-between"><h1 class="text-xl text-bright font-semibold">Live Feed</h1> <div class="flex gap-3"><span class="text-dim text-sm">${escape_html(store_get($$store_subs ??= {}, "$eventFeed", eventFeed).length)} events</span> <button class="text-xs text-muted hover:text-text transition">Clear</button></div></div> `);
    if (store_get($$store_subs ??= {}, "$eventFeed", eventFeed).length === 0) {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="text-center py-20 text-dim"><div class="text-4xl mb-4">◉</div> <p>Waiting for cognitive events...</p> <p class="text-sm text-muted mt-2">Events appear here in real-time as Vestige thinks.</p></div>`);
    } else {
      $$renderer2.push("<!--[!-->");
      $$renderer2.push(`<div class="space-y-2"><!--[-->`);
      const each_array = ensure_array_like(store_get($$store_subs ??= {}, "$eventFeed", eventFeed));
      for (let i = 0, $$length = each_array.length; i < $$length; i++) {
        let event = each_array[i];
        $$renderer2.push(`<div class="flex items-start gap-3 p-3 bg-surface/40 border border-subtle/15 rounded-lg hover:border-subtle/30 transition-all duration-200"${attr_style(`border-left: 3px solid ${stringify(EVENT_TYPE_COLORS[event.type] || "#6b7280")}`)}><div class="w-6 h-6 rounded flex items-center justify-center text-xs flex-shrink-0"${attr_style(`background: ${stringify(EVENT_TYPE_COLORS[event.type] || "#6b7280")}20; color: ${stringify(EVENT_TYPE_COLORS[event.type] || "#6b7280")}`)}>${escape_html(eventIcon(event.type))}</div> <div class="flex-1 min-w-0"><div class="flex items-center gap-2 mb-0.5"><span class="text-xs font-medium"${attr_style(`color: ${stringify(EVENT_TYPE_COLORS[event.type] || "#6b7280")}`)}>${escape_html(event.type)}</span> `);
        if (event.data.timestamp) {
          $$renderer2.push("<!--[-->");
          $$renderer2.push(`<span class="text-xs text-muted">${escape_html(formatTime(String(event.data.timestamp)))}</span>`);
        } else {
          $$renderer2.push("<!--[!-->");
        }
        $$renderer2.push(`<!--]--></div> <p class="text-sm text-dim">${escape_html(eventSummary(event))}</p></div></div>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
    if ($$store_subs) unsubscribe_stores($$store_subs);
  });
}
export {
  _page as default
};
