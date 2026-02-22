import { ad as ssr_context, a as attr, b as attr_class, c as escape_html, s as store_get, f as unsubscribe_stores, d as stringify } from "../../../../chunks/index.js";
import { a as api } from "../../../../chunks/api.js";
import { e as eventFeed } from "../../../../chunks/websocket.js";
function onDestroy(fn) {
  /** @type {SSRContext} */
  ssr_context.r.on_destroy(fn);
}
function Graph3D($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let animationId;
    onDestroy(() => {
      cancelAnimationFrame(animationId);
      window.removeEventListener("resize", onResize);
    });
    function onResize() {
      return;
    }
    $$renderer2.push(`<div class="w-full h-full"></div>`);
  });
}
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    var $$store_subs;
    let graphData = null;
    let loading = true;
    let error = "";
    let isDreaming = false;
    let searchQuery = "";
    let maxNodes = 150;
    async function loadGraph(query, centerId) {
      loading = true;
      error = "";
      try {
        graphData = await api.graph({
          max_nodes: maxNodes,
          depth: 3,
          query: query || void 0,
          center_id: centerId || void 0
        });
      } catch {
        error = "No memories yet. Start using Vestige to populate your graph.";
      } finally {
        loading = false;
      }
    }
    $$renderer2.push(`<div class="h-full relative">`);
    if (loading) {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="h-full flex items-center justify-center"><div class="text-center space-y-4"><div class="w-16 h-16 mx-auto rounded-full border-2 border-synapse/30 border-t-synapse animate-spin"></div> <p class="text-dim text-sm">Loading memory graph...</p></div></div>`);
    } else if (error) {
      $$renderer2.push("<!--[1-->");
      $$renderer2.push(`<div class="h-full flex items-center justify-center"><div class="text-center space-y-4 max-w-md px-8"><div class="text-5xl opacity-30">◎</div> <h2 class="text-xl text-bright">Your Mind Awaits</h2> <p class="text-dim text-sm">${escape_html(error)}</p></div></div>`);
    } else if (graphData) {
      $$renderer2.push("<!--[2-->");
      Graph3D($$renderer2, {
        nodes: graphData.nodes,
        edges: graphData.edges,
        centerId: graphData.center_id,
        events: store_get($$store_subs ??= {}, "$eventFeed", eventFeed)
      });
    } else {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--> <div class="absolute top-4 left-4 right-4 z-10 flex items-center gap-3"><div class="flex gap-2 flex-1 max-w-md"><input type="text" placeholder="Center graph on..."${attr("value", searchQuery)} class="flex-1 px-3 py-2 bg-abyss/80 backdrop-blur-sm border border-subtle/30 rounded-lg text-text text-sm placeholder:text-muted focus:outline-none focus:border-synapse/50 transition"/> <button class="px-3 py-2 bg-synapse/20 border border-synapse/40 text-synapse-glow text-sm rounded-lg hover:bg-synapse/30 transition backdrop-blur-sm">Focus</button></div> <div class="flex gap-2 ml-auto">`);
    $$renderer2.select(
      {
        value: maxNodes,
        onchange: () => loadGraph(),
        class: "px-2 py-2 bg-abyss/80 backdrop-blur-sm border border-subtle/30 rounded-lg text-dim text-xs"
      },
      ($$renderer3) => {
        $$renderer3.option({ value: 50 }, ($$renderer4) => {
          $$renderer4.push(`50 nodes`);
        });
        $$renderer3.option({ value: 100 }, ($$renderer4) => {
          $$renderer4.push(`100 nodes`);
        });
        $$renderer3.option({ value: 150 }, ($$renderer4) => {
          $$renderer4.push(`150 nodes`);
        });
        $$renderer3.option({ value: 200 }, ($$renderer4) => {
          $$renderer4.push(`200 nodes`);
        });
      }
    );
    $$renderer2.push(` <button${attr("disabled", isDreaming, true)}${attr_class(`px-4 py-2 rounded-lg bg-dream/20 border border-dream/40 text-dream-glow text-sm hover:bg-dream/30 transition-all backdrop-blur-sm disabled:opacity-50 ${stringify("")}`)}>${escape_html("◈ Dream")}</button> <button class="px-3 py-2 bg-abyss/80 backdrop-blur-sm border border-subtle/30 rounded-lg text-dim text-sm hover:text-text transition">↻</button></div></div> <div class="absolute bottom-4 left-4 z-10 text-xs text-dim backdrop-blur-sm bg-abyss/60 rounded-lg px-3 py-2 border border-subtle/20">`);
    if (graphData) {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<span>${escape_html(graphData.nodeCount)} nodes</span> <span class="mx-2 text-subtle">·</span> <span>${escape_html(graphData.edgeCount)} edges</span> <span class="mx-2 text-subtle">·</span> <span>depth ${escape_html(graphData.depth)}</span>`);
    } else {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]--></div>`);
    if ($$store_subs) unsubscribe_stores($$store_subs);
  });
}
export {
  _page as default
};
