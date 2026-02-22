export const manifest = (() => {
function __memo(fn) {
	let value;
	return () => value ??= (value = fn());
}

return {
	appDir: "_app",
	appPath: "dashboard/_app",
	assets: new Set(["favicon.svg","manifest.json"]),
	mimeTypes: {".svg":"image/svg+xml",".json":"application/json"},
	_: {
		client: {start:"_app/immutable/entry/start.BdzkYIOY.js",app:"_app/immutable/entry/app.BBPt9AEJ.js",imports:["_app/immutable/entry/start.BdzkYIOY.js","_app/immutable/chunks/rHGvVkdq.js","_app/immutable/chunks/DleE0ac1.js","_app/immutable/chunks/DrTsYth1.js","_app/immutable/chunks/DZf5toYK.js","_app/immutable/entry/app.BBPt9AEJ.js","_app/immutable/chunks/DleE0ac1.js","_app/immutable/chunks/8PSwG_AU.js","_app/immutable/chunks/wmwKEafM.js","_app/immutable/chunks/DZf5toYK.js","_app/immutable/chunks/BHs8FnOA.js","_app/immutable/chunks/BolYP48w.js","_app/immutable/chunks/D6XtQ4nY.js","_app/immutable/chunks/D-x7U94i.js","_app/immutable/chunks/M1z6VHZC.js","_app/immutable/chunks/DrTsYth1.js"],stylesheets:[],fonts:[],uses_env_dynamic_public:false},
		nodes: [
			__memo(() => import('./nodes/0.js')),
			__memo(() => import('./nodes/1.js')),
			__memo(() => import('./nodes/2.js')),
			__memo(() => import('./nodes/3.js')),
			__memo(() => import('./nodes/4.js')),
			__memo(() => import('./nodes/5.js')),
			__memo(() => import('./nodes/6.js')),
			__memo(() => import('./nodes/7.js')),
			__memo(() => import('./nodes/8.js')),
			__memo(() => import('./nodes/9.js')),
			__memo(() => import('./nodes/10.js')),
			__memo(() => import('./nodes/11.js'))
		],
		remotes: {
			
		},
		routes: [
			{
				id: "/",
				pattern: /^\/$/,
				params: [],
				page: { layouts: [0,], errors: [1,], leaf: 3 },
				endpoint: null
			},
			{
				id: "/(app)/explore",
				pattern: /^\/explore\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 4 },
				endpoint: null
			},
			{
				id: "/(app)/feed",
				pattern: /^\/feed\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 5 },
				endpoint: null
			},
			{
				id: "/(app)/graph",
				pattern: /^\/graph\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 6 },
				endpoint: null
			},
			{
				id: "/(app)/intentions",
				pattern: /^\/intentions\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 7 },
				endpoint: null
			},
			{
				id: "/(app)/memories",
				pattern: /^\/memories\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 8 },
				endpoint: null
			},
			{
				id: "/(app)/settings",
				pattern: /^\/settings\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 9 },
				endpoint: null
			},
			{
				id: "/(app)/stats",
				pattern: /^\/stats\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 10 },
				endpoint: null
			},
			{
				id: "/(app)/timeline",
				pattern: /^\/timeline\/?$/,
				params: [],
				page: { layouts: [0,2,], errors: [1,,], leaf: 11 },
				endpoint: null
			}
		],
		prerendered_routes: new Set([]),
		matchers: async () => {
			
			return {  };
		},
		server_assets: {}
	}
}
})();
