import { e as ensure_array_like, c as escape_html, ac as attr_style, d as stringify } from "../../../../chunks/index.js";
import { a as api } from "../../../../chunks/api.js";
import { N as NODE_TYPE_COLORS } from "../../../../chunks/index3.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let timeline = [];
    let loading = true;
    let days = 14;
    let expandedDay = null;
    async function loadTimeline() {
      loading = true;
      try {
        const res = await api.timeline(days, 500);
        timeline = res.timeline;
      } catch {
        timeline = [];
      } finally {
        loading = false;
      }
    }
    $$renderer2.push(`<div class="p-6 max-w-4xl mx-auto space-y-6"><div class="flex items-center justify-between"><h1 class="text-xl text-bright font-semibold">Timeline</h1> `);
    $$renderer2.select(
      {
        value: days,
        onchange: loadTimeline,
        class: "px-3 py-2 bg-surface border border-subtle/40 rounded-lg text-dim text-sm"
      },
      ($$renderer3) => {
        $$renderer3.option({ value: 7 }, ($$renderer4) => {
          $$renderer4.push(`7 days`);
        });
        $$renderer3.option({ value: 14 }, ($$renderer4) => {
          $$renderer4.push(`14 days`);
        });
        $$renderer3.option({ value: 30 }, ($$renderer4) => {
          $$renderer4.push(`30 days`);
        });
        $$renderer3.option({ value: 90 }, ($$renderer4) => {
          $$renderer4.push(`90 days`);
        });
      }
    );
    $$renderer2.push(`</div> `);
    if (loading) {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="space-y-4"><!--[-->`);
      const each_array = ensure_array_like(Array(7));
      for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
        each_array[$$index];
        $$renderer2.push(`<div class="h-16 bg-surface/50 rounded-lg animate-pulse"></div>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    } else if (timeline.length === 0) {
      $$renderer2.push("<!--[1-->");
      $$renderer2.push(`<div class="text-center py-20 text-dim"><p>No memories in the selected time range.</p></div>`);
    } else {
      $$renderer2.push("<!--[!-->");
      $$renderer2.push(`<div class="relative"><div class="absolute left-6 top-0 bottom-0 w-px bg-subtle/30"></div> <div class="space-y-4"><!--[-->`);
      const each_array_1 = ensure_array_like(timeline);
      for (let $$index_3 = 0, $$length = each_array_1.length; $$index_3 < $$length; $$index_3++) {
        let day = each_array_1[$$index_3];
        $$renderer2.push(`<div class="relative pl-14"><div class="absolute left-4 top-3 w-5 h-5 rounded-full border-2 border-synapse bg-abyss flex items-center justify-center"><div class="w-2 h-2 rounded-full bg-synapse"></div></div> <button class="w-full text-left p-4 bg-surface/40 border border-subtle/20 rounded-lg hover:border-synapse/30 transition-all"><div class="flex items-center justify-between"><div><span class="text-sm text-bright font-medium">${escape_html(day.date)}</span> <span class="text-xs text-dim ml-2">${escape_html(day.count)} memories</span></div> <div class="flex gap-1"><!--[-->`);
        const each_array_2 = ensure_array_like(day.memories.slice(0, 10));
        for (let $$index_1 = 0, $$length2 = each_array_2.length; $$index_1 < $$length2; $$index_1++) {
          let m = each_array_2[$$index_1];
          $$renderer2.push(`<div class="w-2 h-2 rounded-full"${attr_style(`background: ${stringify(NODE_TYPE_COLORS[m.nodeType] || "#6b7280")}; opacity: ${stringify(0.3 + m.retentionStrength * 0.7)}`)}></div>`);
        }
        $$renderer2.push(`<!--]--> `);
        if (day.memories.length > 10) {
          $$renderer2.push("<!--[-->");
          $$renderer2.push(`<span class="text-xs text-muted">+${escape_html(day.memories.length - 10)}</span>`);
        } else {
          $$renderer2.push("<!--[!-->");
        }
        $$renderer2.push(`<!--]--></div></div> `);
        if (expandedDay === day.date) {
          $$renderer2.push("<!--[-->");
          $$renderer2.push(`<div class="mt-3 pt-3 border-t border-subtle/20 space-y-2"><!--[-->`);
          const each_array_3 = ensure_array_like(day.memories);
          for (let $$index_2 = 0, $$length2 = each_array_3.length; $$index_2 < $$length2; $$index_2++) {
            let m = each_array_3[$$index_2];
            $$renderer2.push(`<div class="flex items-start gap-2 text-sm"><div class="w-2 h-2 mt-1.5 rounded-full flex-shrink-0"${attr_style(`background: ${stringify(NODE_TYPE_COLORS[m.nodeType] || "#6b7280")}`)}></div> <div class="flex-1 min-w-0"><span class="text-dim line-clamp-1">${escape_html(m.content)}</span></div> <span class="text-xs text-muted flex-shrink-0">${escape_html((m.retentionStrength * 100).toFixed(0))}%</span></div>`);
          }
          $$renderer2.push(`<!--]--></div>`);
        } else {
          $$renderer2.push("<!--[!-->");
        }
        $$renderer2.push(`<!--]--></button></div>`);
      }
      $$renderer2.push(`<!--]--></div></div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
