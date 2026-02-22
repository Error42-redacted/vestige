import { c as escape_html, a as attr, e as ensure_array_like, b as attr_class, d as stringify, ac as attr_style } from "../../../../chunks/index.js";
import { a as api } from "../../../../chunks/api.js";
import { N as NODE_TYPE_COLORS } from "../../../../chunks/index3.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let memories = [];
    let searchQuery = "";
    let selectedType = "";
    let selectedTag = "";
    let minRetention = 0;
    let loading = true;
    let selectedMemory = null;
    async function loadMemories() {
      loading = true;
      try {
        const params = {};
        if (searchQuery) ;
        if (selectedType) ;
        if (selectedTag) ;
        if (minRetention > 0) ;
        const res = await api.memories.list(params);
        memories = res.memories;
      } catch {
        memories = [];
      } finally {
        loading = false;
      }
    }
    function retentionColor(r) {
      if (r > 0.7) return "#10b981";
      if (r > 0.4) return "#f59e0b";
      return "#ef4444";
    }
    $$renderer2.push(`<div class="p-6 max-w-6xl mx-auto space-y-6"><div class="flex items-center justify-between"><h1 class="text-xl text-bright font-semibold">Memories</h1> <span class="text-dim text-sm">${escape_html(memories.length)} results</span></div> <div class="flex gap-3 flex-wrap"><input type="text" placeholder="Search memories..."${attr("value", searchQuery)} class="flex-1 min-w-64 px-4 py-2.5 bg-surface border border-subtle/40 rounded-lg text-text text-sm placeholder:text-muted focus:outline-none focus:border-synapse/60 focus:ring-1 focus:ring-synapse/30 transition"/> `);
    $$renderer2.select(
      {
        value: selectedType,
        onchange: loadMemories,
        class: "px-3 py-2.5 bg-surface border border-subtle/40 rounded-lg text-dim text-sm focus:outline-none"
      },
      ($$renderer3) => {
        $$renderer3.option({ value: "" }, ($$renderer4) => {
          $$renderer4.push(`All types`);
        });
        $$renderer3.option({ value: "fact" }, ($$renderer4) => {
          $$renderer4.push(`Fact`);
        });
        $$renderer3.option({ value: "concept" }, ($$renderer4) => {
          $$renderer4.push(`Concept`);
        });
        $$renderer3.option({ value: "event" }, ($$renderer4) => {
          $$renderer4.push(`Event`);
        });
        $$renderer3.option({ value: "person" }, ($$renderer4) => {
          $$renderer4.push(`Person`);
        });
        $$renderer3.option({ value: "place" }, ($$renderer4) => {
          $$renderer4.push(`Place`);
        });
        $$renderer3.option({ value: "note" }, ($$renderer4) => {
          $$renderer4.push(`Note`);
        });
        $$renderer3.option({ value: "pattern" }, ($$renderer4) => {
          $$renderer4.push(`Pattern`);
        });
        $$renderer3.option({ value: "decision" }, ($$renderer4) => {
          $$renderer4.push(`Decision`);
        });
      }
    );
    $$renderer2.push(` <div class="flex items-center gap-2 text-xs text-dim"><span>Min retention:</span> <input type="range" min="0" max="1" step="0.1"${attr("value", minRetention)} class="w-24 accent-synapse"/> <span>${escape_html((minRetention * 100).toFixed(0))}%</span></div></div> `);
    if (loading) {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="grid gap-3"><!--[-->`);
      const each_array = ensure_array_like(Array(8));
      for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
        each_array[$$index];
        $$renderer2.push(`<div class="h-24 bg-surface/50 rounded-lg animate-pulse"></div>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    } else {
      $$renderer2.push("<!--[!-->");
      $$renderer2.push(`<div class="grid gap-3"><!--[-->`);
      const each_array_1 = ensure_array_like(memories);
      for (let $$index_2 = 0, $$length = each_array_1.length; $$index_2 < $$length; $$index_2++) {
        let memory = each_array_1[$$index_2];
        $$renderer2.push(`<button${attr_class(`text-left p-4 bg-surface/50 border border-subtle/20 rounded-lg hover:border-synapse/30 hover:bg-surface transition-all duration-200 group ${stringify(selectedMemory?.id === memory.id ? "border-synapse/50 glow-synapse" : "")}`)}><div class="flex items-start justify-between gap-4"><div class="flex-1 min-w-0"><div class="flex items-center gap-2 mb-2"><span class="w-2 h-2 rounded-full"${attr_style(`background: ${stringify(NODE_TYPE_COLORS[memory.nodeType] || "#6b7280")}`)}></span> <span class="text-xs text-dim">${escape_html(memory.nodeType)}</span> <!--[-->`);
        const each_array_2 = ensure_array_like(memory.tags.slice(0, 3));
        for (let $$index_1 = 0, $$length2 = each_array_2.length; $$index_1 < $$length2; $$index_1++) {
          let tag = each_array_2[$$index_1];
          $$renderer2.push(`<span class="text-xs px-1.5 py-0.5 bg-deep rounded text-muted">${escape_html(tag)}</span>`);
        }
        $$renderer2.push(`<!--]--></div> <p class="text-sm text-text leading-relaxed line-clamp-2">${escape_html(memory.content)}</p></div> <div class="flex flex-col items-end gap-1 flex-shrink-0"><div class="w-12 h-1.5 bg-deep rounded-full overflow-hidden"><div class="h-full rounded-full"${attr_style(`width: ${stringify(memory.retentionStrength * 100)}%; background: ${stringify(retentionColor(memory.retentionStrength))}`)}></div></div> <span class="text-xs text-muted">${escape_html((memory.retentionStrength * 100).toFixed(0))}%</span></div></div> `);
        if (selectedMemory?.id === memory.id) {
          $$renderer2.push("<!--[-->");
          $$renderer2.push(`<div class="mt-4 pt-4 border-t border-subtle/20 space-y-3"><p class="text-sm text-text whitespace-pre-wrap">${escape_html(memory.content)}</p> <div class="grid grid-cols-3 gap-3 text-xs text-dim"><div>Storage: ${escape_html((memory.storageStrength * 100).toFixed(1))}%</div> <div>Retrieval: ${escape_html((memory.retrievalStrength * 100).toFixed(1))}%</div> <div>Created: ${escape_html(new Date(memory.createdAt).toLocaleDateString())}</div></div> <div class="flex gap-2"><button class="px-3 py-1.5 bg-recall/20 text-recall text-xs rounded hover:bg-recall/30">Promote</button> <button class="px-3 py-1.5 bg-decay/20 text-decay text-xs rounded hover:bg-decay/30">Demote</button> <button class="px-3 py-1.5 bg-decay/10 text-decay/60 text-xs rounded hover:bg-decay/20 ml-auto">Delete</button></div></div>`);
        } else {
          $$renderer2.push("<!--[!-->");
        }
        $$renderer2.push(`<!--]--></button>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
