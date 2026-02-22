import { e as ensure_array_like } from "../../../../chunks/index.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    $$renderer2.push(`<div class="p-6 max-w-5xl mx-auto space-y-6"><h1 class="text-xl text-bright font-semibold">System Stats</h1> `);
    {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="grid grid-cols-2 lg:grid-cols-4 gap-4"><!--[-->`);
      const each_array = ensure_array_like(Array(8));
      for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
        each_array[$$index];
        $$renderer2.push(`<div class="h-24 bg-surface/50 rounded-lg animate-pulse"></div>`);
      }
      $$renderer2.push(`<!--]--></div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
