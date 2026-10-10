import './app/styles.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { followChanges } from './app/change-feed';
import { App } from './app/providers';
import { queryClient } from './app/query-client';
import { router } from './app/router';
import { QUIT_REQUESTED } from './entities/setup/queries';
import { changeLanguage, startI18n } from './shared/i18n/i18n';
import { commands, events } from './shared/ipc';

async function start(root: HTMLElement): Promise<void> {
	if (import.meta.env.MODE === 'mock') {
		await import('./app/mock/ipc');
	}
	await followChanges(queryClient);
	await events.languageChanged.listen(({ payload }) => {
		changeLanguage(payload).then(() => queryClient.invalidateQueries());
	});
	await events.quitRequested.listen(() => {
		queryClient.setQueryData(QUIT_REQUESTED, true);
	});
	await events.navigate.listen(() => {
		router.navigate({ to: '/' });
	});
	await startI18n(await commands.getLocale());
	createRoot(root).render(
		<StrictMode>
			<App />
		</StrictMode>,
	);
}

const root = document.getElementById('root');
if (root) {
	start(root);
}
