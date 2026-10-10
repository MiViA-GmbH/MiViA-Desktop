import { mockIPC } from '@tauri-apps/api/mocks';

import type { FolderView } from '../../shared/ipc/bindings';
import {
	ATTENTION,
	BATCH,
	FOLDERS,
	HOLDS,
	ITEMS,
	NETWORK,
	NETWORK_ATTENTION,
	NETWORK_HOLDS,
	ONE_OFF,
	REPORTS,
} from './fixtures';
import { historyResponses } from './history';
import { privacyResponses } from './privacy';
import { setupResponses, setupView } from './setup';
import { supportResponses } from './support';

const params = new URLSearchParams(window.location.search);
const scenario = params.get('scenario') ?? 'hold';
const language = params.get('lang') === 'de' ? 'de' : 'en';

function folders(): FolderView[] {
	if (scenario === 'empty') {
		return [];
	}
	if (scenario === 'paused') {
		return FOLDERS.slice(0, 4).map((folder) => ({ ...folder, paused: true, openSeries: null }));
	}
	if (scenario === 'network') {
		return FOLDERS.slice(0, 4).map((folder) => ({
			...folder,
			waitingSince: folder.id === '3' ? '2026-10-12T08:30:00.000Z' : null,
		}));
	}
	return scenario === 'calm' ? FOLDERS.slice(0, 4) : FOLDERS;
}

const calm = scenario !== 'hold';

function holds() {
	if (scenario === 'network') {
		return NETWORK_HOLDS;
	}
	return calm ? { revision: '9', holds: [] } : HOLDS;
}

function attention() {
	if (scenario === 'network') {
		return NETWORK_ATTENTION;
	}
	return calm ? { revision: '9', total: 0, items: [] } : ATTENTION;
}

const responses: Record<string, unknown> = {
	get_locale: { language, setting: 'auto', formatLocale: `${language}-DE` },
	get_account: {
		displayName: 'Dr. Katrin Möller',
		serverUrl: 'https://app.mivia.ai',
		serverAccountId: 'a1',
		email: 'k.moeller@kessler-werkstoff.de',
	},
	list_folders: { revision: '9', folders: folders() },
	list_holds: holds(),
	list_needs_attention: attention(),
	get_network: NETWORK,
	get_network_details: 'Host: app.mivia.ai',
	get_batch: BATCH,
	list_batch_items: ITEMS,
	list_series_reports: REPORTS,
	get_one_off: scenario === 'one-off' ? ONE_OFF : null,
	get_remaining_uploads: 1176,
	...supportResponses(scenario),
};

const setup = {
	...setupResponses(setupView(scenario)),
	...historyResponses(scenario),
	...privacyResponses(scenario),
};

mockIPC((command, args) => setup[command]?.((args ?? {}) as Record<string, unknown>) ?? responses[command] ?? null, {
	shouldMockEvents: true,
});
