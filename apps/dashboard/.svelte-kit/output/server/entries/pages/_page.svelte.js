import { a as attr, b as attr_class, c as escape_html, d as stringify } from "../../chunks/index.js";
import "../../chunks/websocket.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let isDreaming = false;
    $$renderer2.push(`<div class="h-full relative">`);
    {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="h-full flex items-center justify-center"><div class="text-center space-y-4"><div class="w-16 h-16 mx-auto rounded-full border-2 border-synapse/30 border-t-synapse animate-spin"></div> <p class="text-dim text-sm">Loading memory graph...</p></div></div>`);
    }
    $$renderer2.push(`<!--]--> <div class="absolute top-4 left-4 flex gap-2 z-10"><button${attr("disabled", isDreaming, true)}${attr_class(`px-4 py-2 rounded-lg bg-dream/20 border border-dream/40 text-dream-glow text-sm hover:bg-dream/30 transition-all disabled:opacity-50 backdrop-blur-sm ${stringify("")}`)}>${escape_html("◎ Dream")}</button></div> <div class="absolute top-4 right-4 z-10 text-xs text-dim backdrop-blur-sm bg-abyss/60 rounded-lg px-3 py-2 border border-subtle/20">`);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
