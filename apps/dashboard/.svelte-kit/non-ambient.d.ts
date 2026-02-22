
// this file is generated — do not edit it


declare module "svelte/elements" {
	export interface HTMLAttributes<T> {
		'data-sveltekit-keepfocus'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-noscroll'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-preload-code'?:
			| true
			| ''
			| 'eager'
			| 'viewport'
			| 'hover'
			| 'tap'
			| 'off'
			| undefined
			| null;
		'data-sveltekit-preload-data'?: true | '' | 'hover' | 'tap' | 'off' | undefined | null;
		'data-sveltekit-reload'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-replacestate'?: true | '' | 'off' | undefined | null;
	}
}

export {};


declare module "$app/types" {
	export interface AppTypes {
		RouteId(): "/(app)" | "/" | "/(app)/explore" | "/(app)/feed" | "/(app)/graph" | "/(app)/intentions" | "/(app)/memories" | "/(app)/settings" | "/(app)/stats" | "/(app)/timeline";
		RouteParams(): {
			
		};
		LayoutParams(): {
			"/(app)": Record<string, never>;
			"/": Record<string, never>;
			"/(app)/explore": Record<string, never>;
			"/(app)/feed": Record<string, never>;
			"/(app)/graph": Record<string, never>;
			"/(app)/intentions": Record<string, never>;
			"/(app)/memories": Record<string, never>;
			"/(app)/settings": Record<string, never>;
			"/(app)/stats": Record<string, never>;
			"/(app)/timeline": Record<string, never>
		};
		Pathname(): "/" | "/explore" | "/feed" | "/graph" | "/intentions" | "/memories" | "/settings" | "/stats" | "/timeline";
		ResolvedPathname(): `${"" | `/${string}`}${ReturnType<AppTypes['Pathname']>}`;
		Asset(): "/favicon.svg" | "/manifest.json" | string & {};
	}
}