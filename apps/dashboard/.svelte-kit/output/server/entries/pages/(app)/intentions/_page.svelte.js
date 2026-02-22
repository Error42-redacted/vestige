import { c as escape_html, e as ensure_array_like, b as attr_class, d as stringify } from "../../../../chunks/index.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let intentions = [];
    let predictions = [];
    let statusFilter = "active";
    $$renderer2.push(`<div class="p-6 max-w-5xl mx-auto space-y-8"><div class="flex items-center justify-between"><h1 class="text-xl text-bright font-semibold">Intentions &amp; Predictions</h1> <span class="text-xs text-muted">${escape_html(intentions.length)} intentions</span></div> <div class="space-y-4"><div class="flex items-center gap-2"><h2 class="text-sm text-bright font-semibold">Prospective Memory</h2> <span class="text-xs text-muted">"Remember to do X when Y happens"</span></div> <div class="flex gap-1.5"><!--[-->`);
    const each_array = ensure_array_like(["active", "fulfilled", "snoozed", "cancelled", "all"]);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let status = each_array[$$index];
      $$renderer2.push(`<button${attr_class(`px-3 py-1.5 rounded-lg text-xs transition ${stringify(statusFilter === status ? "bg-synapse/20 text-synapse-glow border border-synapse/40" : "bg-surface/40 text-dim border border-subtle/20 hover:border-subtle/40")}`)}>${escape_html(status.charAt(0).toUpperCase() + status.slice(1))}</button>`);
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="space-y-2"><!--[-->`);
      const each_array_1 = ensure_array_like(Array(4));
      for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
        each_array_1[$$index_1];
        $$renderer2.push(`<div class="h-16 bg-surface/50 rounded-lg animate-pulse"></div>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    }
    $$renderer2.push(`<!--]--></div> <div class="pt-6 border-t border-subtle/20 space-y-4"><div class="flex items-center gap-2"><h2 class="text-sm text-bright font-semibold">Predicted Needs</h2> <span class="text-xs text-muted">What you might need next</span></div> `);
    if (predictions.length === 0) {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="text-center py-8 text-dim"><div class="text-3xl mb-3 opacity-20">◬</div> <p class="text-sm">No predictions yet. Use Vestige more to train the predictive model.</p></div>`);
    } else {
      $$renderer2.push("<!--[!-->");
      $$renderer2.push(`<div class="space-y-2"><!--[-->`);
      const each_array_3 = ensure_array_like(predictions);
      for (let i = 0, $$length = each_array_3.length; i < $$length; i++) {
        let pred = each_array_3[i];
        $$renderer2.push(`<div class="p-3 bg-surface/40 border border-subtle/20 rounded-lg flex items-start gap-3"><div class="w-6 h-6 rounded-full bg-dream/20 text-dream-glow text-xs flex items-center justify-center flex-shrink-0 mt-0.5">${escape_html(i + 1)}</div> <div class="flex-1 min-w-0"><p class="text-sm text-text line-clamp-2">${escape_html(pred.content)}</p> <div class="flex gap-3 mt-1 text-xs text-muted"><span>${escape_html(pred.nodeType)}</span> `);
        if (pred.retention) {
          $$renderer2.push("<!--[-->");
          $$renderer2.push(`<span>${escape_html((Number(pred.retention) * 100).toFixed(0))}% retention</span>`);
        } else {
          $$renderer2.push("<!--[!-->");
        }
        $$renderer2.push(`<!--]--> `);
        if (pred.predictedNeed) {
          $$renderer2.push("<!--[-->");
          $$renderer2.push(`<span class="text-dream-glow">${escape_html(pred.predictedNeed)} need</span>`);
        } else {
          $$renderer2.push("<!--[!-->");
        }
        $$renderer2.push(`<!--]--></div></div></div>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    }
    $$renderer2.push(`<!--]--></div></div>`);
  });
}
export {
  _page as default
};
