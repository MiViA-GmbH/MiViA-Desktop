import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [react()],
	clearScreen: false,
	server: { port: 5173, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
	preview: { port: 4173, strictPort: true },
	envPrefix: ['VITE_', 'TAURI_ENV_*'],
	build: {
		target: 'chrome120',
		modulePreload: { polyfill: false },
		rollupOptions: {
			output: {
				entryFileNames: 'assets/startup-[hash].js',
				chunkFileNames: (chunk) => (chunk.isDynamicEntry ? 'assets/lazy-[name]-[hash].js' : 'assets/[name]-[hash].js'),
			},
		},
	},
});
