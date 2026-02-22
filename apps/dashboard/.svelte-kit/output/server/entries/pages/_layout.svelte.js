import { g as getContext, e as ensure_array_like, s as store_get, a as attr, b as attr_class, c as escape_html, d as stringify, f as unsubscribe_stores } from "../../chunks/index.js";
import "@sveltejs/kit/internal";
import "../../chunks/exports.js";
import "../../chunks/utils2.js";
import "@sveltejs/kit/internal/server";
import "../../chunks/root.js";
import "../../chunks/state.svelte.js";
import { b as base } from "../../chunks/server.js";
import { i as isConnected, m as memoryCount, a as avgRetention } from "../../chunks/websocket.js";
const getStores = () => {
  const stores$1 = getContext("__svelte__");
  return {
    /** @type {typeof page} */
    page: {
      subscribe: stores$1.page.subscribe
    },
    /** @type {typeof navigating} */
    navigating: {
      subscribe: stores$1.navigating.subscribe
    },
    /** @type {typeof updated} */
    updated: stores$1.updated
  };
};
const page = {
  subscribe(fn) {
    const store = getStores().page;
    return store.subscribe(fn);
  }
};
function _layout($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    var $$store_subs;
    let { children } = $$props;
    const nav = [
      { href: "/", label: "Graph", icon: "◎", shortcut: "G" },
      {
        href: "/memories",
        label: "Memories",
        icon: "◈",
        shortcut: "M"
      },
      {
        href: "/timeline",
        label: "Timeline",
        icon: "◷",
        shortcut: "T"
      },
      { href: "/feed", label: "Feed", icon: "◉", shortcut: "F" },
      { href: "/explore", label: "Explore", icon: "◬", shortcut: "E" },
      {
        href: "/intentions",
        label: "Intentions",
        icon: "◇",
        shortcut: "I"
      },
      { href: "/stats", label: "Stats", icon: "◫", shortcut: "S" },
      {
        href: "/settings",
        label: "Settings",
        icon: "⚙",
        shortcut: ","
      }
    ];
    const mobileNav = nav.slice(0, 5);
    function isActive(href, currentPath) {
      const path = currentPath.startsWith(base) ? currentPath.slice(base.length) || "/" : currentPath;
      if (href === "/") return path === "/" || path === "/graph";
      return path.startsWith(href);
    }
    $$renderer2.push(`<div class="flex flex-col md:flex-row h-screen overflow-hidden bg-void"><nav class="hidden md:flex w-16 lg:w-56 flex-shrink-0 bg-abyss border-r border-subtle/30 flex-col"><a href="/" class="flex items-center gap-3 px-4 py-5 border-b border-subtle/20"><div class="w-8 h-8 rounded-lg bg-gradient-to-br from-dream to-synapse flex items-center justify-center text-bright text-sm font-bold">V</div> <span class="hidden lg:block text-sm font-semibold text-bright tracking-wide">VESTIGE</span></a> <div class="flex-1 py-3 flex flex-col gap-1 px-2"><!--[-->`);
    const each_array = ensure_array_like(nav);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let item = each_array[$$index];
      const active = isActive(item.href, store_get($$store_subs ??= {}, "$page", page).url.pathname);
      $$renderer2.push(`<a${attr("href", item.href)}${attr_class(`flex items-center gap-3 px-3 py-2.5 rounded-lg transition-all duration-200 text-sm ${stringify(active ? "bg-synapse/15 text-synapse-glow border border-synapse/30 shadow-[0_0_12px_rgba(99,102,241,0.15)]" : "text-dim hover:text-text hover:bg-surface border border-transparent")}`)}><span class="text-base w-5 text-center">${escape_html(item.icon)}</span> <span class="hidden lg:block">${escape_html(item.label)}</span> <span class="hidden lg:block ml-auto text-[10px] text-muted/50 font-mono">${escape_html(item.shortcut)}</span></a>`);
    }
    $$renderer2.push(`<!--]--></div> <div class="px-2 pb-2"><button class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-xs text-muted hover:text-dim hover:bg-surface/50 transition border border-subtle/20"><span class="text-[10px] font-mono bg-surface/60 px-1.5 py-0.5 rounded">⌘K</span> <span class="hidden lg:block">Command</span></button></div> <div class="px-3 py-4 border-t border-subtle/20 space-y-2"><div class="flex items-center gap-2 text-xs"><div${attr_class(`w-2 h-2 rounded-full ${stringify(store_get($$store_subs ??= {}, "$isConnected", isConnected) ? "bg-recall animate-pulse-glow" : "bg-decay")}`)}></div> <span class="hidden lg:block text-dim">${escape_html(store_get($$store_subs ??= {}, "$isConnected", isConnected) ? "Connected" : "Offline")}</span></div> <div class="hidden lg:block text-xs text-muted"><div>${escape_html(store_get($$store_subs ??= {}, "$memoryCount", memoryCount))} memories</div> <div>${escape_html((store_get($$store_subs ??= {}, "$avgRetention", avgRetention) * 100).toFixed(0))}% retention</div></div></div></nav> <main class="flex-1 overflow-y-auto pb-16 md:pb-0"><div class="animate-page-in svelte-12qhfyh">`);
    children($$renderer2);
    $$renderer2.push(`<!----></div></main> <nav class="md:hidden fixed bottom-0 inset-x-0 bg-abyss/95 backdrop-blur-xl border-t border-subtle/30 z-40 safe-bottom svelte-12qhfyh"><div class="flex items-center justify-around px-2 py-1"><!--[-->`);
    const each_array_1 = ensure_array_like(mobileNav);
    for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
      let item = each_array_1[$$index_1];
      const active = isActive(item.href, store_get($$store_subs ??= {}, "$page", page).url.pathname);
      $$renderer2.push(`<a${attr("href", item.href)}${attr_class(`flex flex-col items-center gap-0.5 px-3 py-2 rounded-lg transition-all min-w-[3.5rem] ${stringify(active ? "text-synapse-glow" : "text-muted")}`)}><span class="text-lg">${escape_html(item.icon)}</span> <span class="text-[9px]">${escape_html(item.label)}</span></a>`);
    }
    $$renderer2.push(`<!--]--> <button class="flex flex-col items-center gap-0.5 px-3 py-2 rounded-lg text-muted min-w-[3.5rem]"><span class="text-lg">⋯</span> <span class="text-[9px]">More</span></button></div></nav></div> `);
    {
      $$renderer2.push("<!--[!-->");
    }
    $$renderer2.push(`<!--]-->`);
    if ($$store_subs) unsubscribe_stores($$store_subs);
  });
}
export {
  _layout as default
};
