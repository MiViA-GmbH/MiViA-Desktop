import { events } from '../../shared/ipc';
import type { UpdateView } from '../../shared/ipc/bindings';

const CURRENT: UpdateView = {
	current: '2.0.0',
	channel: 'stable',
	automatic: true,
	policy: null,
	state: 'upToDate',
	version: null,
	lastCheckAt: '2026-10-12T08:30:00.000Z',
	failedInstall: null,
	notice: false,
	required: null,
	progress: null,
};

const SCENARIOS: Record<string, Partial<UpdateView>> = {
	'update-ready': { state: 'ready', version: '2.1.0', failedInstall: '2.1.0' },
	'update-unsigned': { required: { min: '2.1.0', signed: false } },
	'update-beta-build': { current: '2.0.0-beta.6', channel: 'beta' },
	'update-notice-available': { state: 'available', version: '2.1.0', notice: true },
	'update-notice-ready': { state: 'ready', version: '2.1.0', notice: true },
	'update-locked': { automatic: false, policy: 'off' },
	'update-available': { state: 'available', version: '2.1.0' },
	'update-downloading': { state: 'downloading', version: '2.1.0', progress: 40 },
	'update-checking': { state: 'checking' },
	'update-failed': { state: 'failed' },
};

function updateView(scenario: string): UpdateView {
	return { ...CURRENT, ...SCENARIOS[scenario] };
}

export function updateResponses(scenario: string): Record<string, (args: Record<string, unknown>) => unknown> {
	let view = updateView(scenario);
	let announce = view.notice;
	return {
		get_update: () => {
			if (announce) {
				announce = false;
				setTimeout(() => events.updateNotice.emit({ version: view.version ?? '', ready: view.state === 'ready' }));
			}
			return view;
		},
		check_for_updates: () => view,
		install_update: () => view,
		mark_update_seen: () => null,
		set_update_settings: (args) => {
			view = { ...view, ...(args.input as Pick<UpdateView, 'channel' | 'automatic'>) };
			return view;
		},
	};
}
