import { e as ensure_array_like, b as attr_class, c as escape_html, a as attr, d as stringify } from "../../../../chunks/index.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let searchQuery = "";
    let mode = "associations";
    let importanceText = "";
    const MODE_INFO = {
      associations: {
        icon: "◎",
        desc: "Spreading activation — find related memories via graph traversal"
      },
      chains: {
        icon: "⟿",
        desc: "Build reasoning path from source to target memory"
      },
      bridges: {
        icon: "⬡",
        desc: "Find connecting memories between two concepts"
      }
    };
    $$renderer2.push(`<div class="p-6 max-w-5xl mx-auto space-y-8"><h1 class="text-xl text-bright font-semibold">Explore Connections</h1> <div class="grid grid-cols-3 gap-2"><!--[-->`);
    const each_array = ensure_array_like(["associations", "chains", "bridges"]);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let m = each_array[$$index];
      $$renderer2.push(`<button${attr_class(`flex flex-col items-center gap-1 p-3 rounded-lg text-sm transition ${stringify(mode === m ? "bg-synapse/15 text-synapse-glow border border-synapse/40" : "bg-surface/30 text-dim border border-subtle/20 hover:border-subtle/40")}`)}><span class="text-xl">${escape_html(MODE_INFO[m].icon)}</span> <span class="font-medium">${escape_html(m.charAt(0).toUpperCase() + m.slice(1))}</span> <span class="text-[10px] text-muted text-center">${escape_html(MODE_INFO[m].desc)}</span></button>`);
    }
    $$renderer2.push(`<!--]--></div> <div class="space-y-3"><label class="text-xs text-dim font-medium">Source Memory</label> <div class="flex gap-2"><input type="text" placeholder="Search for a memory to explore from..."${attr("value", searchQuery)} class="flex-1 px-4 py-2.5 bg-surface border border-subtle/40 rounded-lg text-text text-sm placeholder:text-muted focus:outline-none focus:border-synapse/60 transition"/> <button class="px-4 py-2.5 bg-synapse/20 border border-synapse/40 text-synapse-glow text-sm rounded-lg hover:bg-synapse/30 transition">Find</button></div></div> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--> <div class="pt-8 border-t border-subtle/20"><h2 class="text-lg text-bright font-semibold mb-4">Importance Scorer</h2> <p class="text-xs text-muted mb-3">4-channel neuroscience scoring: novelty, arousal, reward, attention</p> <textarea placeholder="Paste any text to score its importance..." class="w-full h-24 px-4 py-3 bg-surface border border-subtle/40 rounded-lg text-text text-sm placeholder:text-muted resize-none focus:outline-none focus:border-synapse/60 transition">`);
    const $$body = escape_html(importanceText);
    if ($$body) {
      $$renderer2.push(`${$$body}`);
    }
    $$renderer2.push(`</textarea> <button class="mt-2 px-4 py-2 bg-dream/20 border border-dream/40 text-dream-glow text-sm rounded-lg hover:bg-dream/30 transition">Score</button> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div></div>`);
  });
}
export {
  _page as default
};
