import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	server: {
		port: 5173,
		proxy: {
			'/api': {
				target: 'http://127.0.0.1:3927',
				changeOrigin: true
			},
			'/ws': {
				target: 'ws://127.0.0.1:3927',
				ws: true
			}
		}
	}
});
