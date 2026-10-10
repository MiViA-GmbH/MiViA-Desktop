import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: 'e2e',
	retries: 0,
	forbidOnly: true,
	workers: 1,
	reporter: 'list',
	use: {
		channel: 'msedge',
		viewport: { width: 1100, height: 688 },
		timezoneId: 'Europe/Berlin',
		locale: 'en-US',
		reducedMotion: 'reduce',
	},
	projects: [
		{
			name: 'smoke',
			testMatch: ['smoke.spec.ts', 'wizard.spec.ts'],
			use: { baseURL: 'http://localhost:4319' },
		},
		{ name: 'e2e', testMatch: 'scenarios.spec.ts', timeout: 240_000 },
	],
	webServer: {
		command: 'vite build --mode mock --outDir dist-mock && vite preview --outDir dist-mock --port 4319 --strictPort',
		url: 'http://localhost:4319',
		reuseExistingServer: false,
	},
});
