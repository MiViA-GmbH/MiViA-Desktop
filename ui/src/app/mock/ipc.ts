import { mockIPC } from '@tauri-apps/api/mocks';

import type { AppError, FolderView, HoldKind } from '../../shared/ipc/bindings';
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
import { previewResponses } from './preview';
import { privacyResponses } from './privacy';
import { setupResponses, setupView } from './setup';
import { supportResponses } from './support';
import { updateResponses } from './update';

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
			waitingSince: folder.id === '2' ? '2026-10-12T08:30:00.000Z' : null,
		}));
	}
	return scenario === 'calm' ? FOLDERS.slice(0, 4) : FOLDERS;
}

const calm = scenario !== 'hold';

const HOLD_KINDS: HoldKind[] = [
	'serviceUnavailable',
	'accessKey',
	'accountBlocked',
	'adminScope',
	'missingScopes',
	'license',
];
const held = HOLD_KINDS.find((kind) => scenario === `hold-${kind}`);

const FAILURE: AppError = {
	kind: 'notFound',
	code: 'reportMissing',
	message: 'report missing',
	signature: 'open-report-missing',
	reportable: true,
	traceId: null,
	computationId: null,
	fields: { name: 'Probe 4709 42CrMo4 Längsschliff.pdf' },
};

function holds() {
	if (held) {
		return { revision: '9', holds: [{ kind: held, since: '2026-10-12T08:42:00.000Z' }] };
	}
	if (scenario === 'network') {
		return NETWORK_HOLDS;
	}
	if (scenario === 'too-old') {
		return { revision: '9', holds: [{ kind: 'clientTooOld' as const, since: '2026-10-12T08:42:00.000Z' }] };
	}
	return calm ? { revision: '9', holds: [] } : HOLDS;
}

function attention() {
	if (scenario === 'network') {
		return NETWORK_ATTENTION;
	}
	return calm ? { revision: '9', total: 0, items: [] } : ATTENTION;
}

const ACCOUNT = {
	displayName: 'Dr. Katrin Möller',
	serverUrl: 'https://app.mivia.ai',
	serverAccountId: 'a1',
	email: 'k.moeller@kessler-werkstoff.de',
};

const responses: Record<string, unknown> = {
	get_locale: { language, setting: 'auto', formatLocale: `${language}-DE` },
	get_account: scenario === 'no-account' ? null : ACCOUNT,
	list_folders: { revision: '9', folders: folders() },
	list_holds: holds(),
	list_needs_attention: attention(),
	get_network: NETWORK,
	get_network_details: 'Host: app.mivia.ai',
	get_batch: scenario === 'renamable' ? { ...BATCH, renamable: true } : BATCH,
	list_batch_items: ITEMS,
	list_series_reports: REPORTS,
	get_one_off: scenario === 'one-off' ? ONE_OFF : null,
	get_remaining_uploads: 1176,
	...supportResponses(scenario),
	claim_report_offer: scenario === 'failure',
};

const setup = {
	...setupResponses(setupView(scenario)),
	...historyResponses(scenario, language),
	...previewResponses(scenario, language),
	...privacyResponses(scenario),
	...updateResponses(scenario),
};

mockIPC(
	(command, args) => {
		if (scenario === 'failure' && (command === 'open_target' || command === 'delete_history')) {
			throw FAILURE;
		}
		return setup[command]?.((args ?? {}) as Record<string, unknown>) ?? responses[command] ?? null;
	},
	{
		shouldMockEvents: true,
	},
);

Object.assign(Reflect.get(window, '__TAURI_INTERNALS__'), { convertFileSrc: (path: string) => path });
