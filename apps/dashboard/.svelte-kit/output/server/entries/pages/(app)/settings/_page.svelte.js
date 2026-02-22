import { c as escape_html, s as store_get, ac as attr_style, d as stringify, b as attr_class, a as attr, e as ensure_array_like, f as unsubscribe_stores } from "../../../../chunks/index.js";
import { m as memoryCount, a as avgRetention, i as isConnected } from "../../../../chunks/websocket.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    var $$store_subs;
    let consolidating = false;
    let dreaming = false;
    $$renderer2.push(`<div class="p-6 max-w-4xl mx-auto space-y-8"><div class="flex items-center justify-between"><h1 class="text-xl text-bright font-semibold">Settings &amp; System</h1> <button class="text-xs text-dim hover:text-text transition">Refresh</button></div> <div class="grid grid-cols-2 md:grid-cols-4 gap-3"><div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg text-center"><div class="text-2xl text-bright font-bold">${escape_html(store_get($$store_subs ??= {}, "$memoryCount", memoryCount))}</div> <div class="text-xs text-dim mt-1">Memories</div></div> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg text-center"><div class="text-2xl font-bold"${attr_style(`color: ${stringify(store_get($$store_subs ??= {}, "$avgRetention", avgRetention) > 0.7 ? "#10b981" : store_get($$store_subs ??= {}, "$avgRetention", avgRetention) > 0.4 ? "#f59e0b" : "#ef4444")}`)}>${escape_html((store_get($$store_subs ??= {}, "$avgRetention", avgRetention) * 100).toFixed(1))}%</div> <div class="text-xs text-dim mt-1">Avg Retention</div></div> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg text-center"><div class="text-2xl text-bright font-bold flex items-center justify-center gap-2"><div${attr_class(`w-2.5 h-2.5 rounded-full ${stringify(store_get($$store_subs ??= {}, "$isConnected", isConnected) ? "bg-recall animate-pulse-glow" : "bg-decay")}`)}></div> <span class="text-sm">${escape_html(store_get($$store_subs ??= {}, "$isConnected", isConnected) ? "Online" : "Offline")}</span></div> <div class="text-xs text-dim mt-1">WebSocket</div></div> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg text-center"><div class="text-2xl text-synapse-glow font-bold">v2.0</div> <div class="text-xs text-dim mt-1">Vestige</div></div></div> <section class="space-y-4"><h2 class="text-sm text-bright font-semibold flex items-center gap-2"><span class="text-dream">◈</span> Cognitive Operations</h2> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg space-y-3"><div class="flex items-center justify-between"><div><div class="text-sm text-text font-medium">FSRS-6 Consolidation</div> <div class="text-xs text-dim">Apply spaced-repetition decay, regenerate embeddings, run maintenance</div></div> <button${attr("disabled", consolidating, true)} class="px-4 py-2 bg-warning/20 border border-warning/40 text-warning text-sm rounded-lg hover:bg-warning/30 transition disabled:opacity-50 flex items-center gap-2">`);
    {
      $$renderer2.push("<!--[!-->");
      $$renderer2.push(`Consolidate`);
    }
    $$renderer2.push(`<!--]--></button></div> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg space-y-3"><div class="flex items-center justify-between"><div><div class="text-sm text-text font-medium">Memory Dream Cycle</div> <div class="text-xs text-dim">Replay memories, discover hidden connections, synthesize insights</div></div> <button${attr("disabled", dreaming, true)}${attr_class(`px-4 py-2 bg-dream/20 border border-dream/40 text-dream-glow text-sm rounded-lg hover:bg-dream/30 transition disabled:opacity-50 flex items-center gap-2 ${stringify("")}`)}>`);
    {
      $$renderer2.push("<!--[!-->");
      $$renderer2.push(`Dream`);
    }
    $$renderer2.push(`<!--]--></button></div> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div></section> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--> <section class="space-y-4"><h2 class="text-sm text-bright font-semibold flex items-center gap-2"><span class="text-synapse">⌨</span> Keyboard Shortcuts</h2> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg"><div class="grid grid-cols-2 gap-2 text-xs"><!--[-->`);
    const each_array_2 = ensure_array_like([
      { key: "⌘ K", desc: "Command palette" },
      { key: "/", desc: "Focus search" },
      { key: "G", desc: "Go to Graph" },
      { key: "M", desc: "Go to Memories" },
      { key: "T", desc: "Go to Timeline" },
      { key: "F", desc: "Go to Feed" },
      { key: "E", desc: "Go to Explore" },
      { key: "S", desc: "Go to Stats" }
    ]);
    for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
      let shortcut = each_array_2[$$index_2];
      $$renderer2.push(`<div class="flex items-center gap-2 py-1"><kbd class="px-1.5 py-0.5 bg-deep rounded text-[10px] font-mono text-muted min-w-[2rem] text-center">${escape_html(shortcut.key)}</kbd> <span class="text-dim">${escape_html(shortcut.desc)}</span></div>`);
    }
    $$renderer2.push(`<!--]--></div></div></section> <section class="space-y-4"><h2 class="text-sm text-bright font-semibold flex items-center gap-2"><span class="text-memory">◎</span> About</h2> <div class="p-4 bg-surface/30 border border-subtle/20 rounded-lg space-y-3"><div class="flex items-center gap-4"><div class="w-12 h-12 rounded-xl bg-gradient-to-br from-dream to-synapse flex items-center justify-center text-bright text-xl font-bold shadow-lg shadow-synapse/20">V</div> <div><div class="text-sm text-bright font-semibold">Vestige v2.0 "Cognitive Leap"</div> <div class="text-xs text-dim">Your AI's long-term memory system</div></div></div> <div class="grid grid-cols-2 gap-2 text-xs text-dim pt-2 border-t border-subtle/10"><div>29 cognitive modules</div> <div>FSRS-6 spaced repetition</div> <div>Nomic Embed v1.5 (256d)</div> <div>Jina Reranker v1 Turbo</div> <div>USearch HNSW (20x FAISS)</div> <div>Local-first, zero cloud</div></div> <div class="text-[10px] text-muted pt-1">Built with Rust + Axum + SvelteKit 2 + Svelte 5 + Three.js + Tailwind CSS 4</div></div></section></div>`);
    if ($$store_subs) unsubscribe_stores($$store_subs);
  });
}
export {
  _page as default
};
