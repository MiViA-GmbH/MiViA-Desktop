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
	applied: null,
	required: null,
	progress: null,
};

const SCENARIOS: Record<string, Partial<UpdateView>> = {
	'update-ready': { state: 'ready', version: '2.1.0', failedInstall: '2.1.0' },
	'update-unsigned': { required: { min: '2.1.0', signed: false } },
	'update-beta-build': { current: '2.0.0-beta.6', channel: 'beta' },
	'update-applied': { applied: '2.0.0' },
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
	return {
		get_update: () => view,
		check_for_updates: () => view,
		install_update: () => view,
		dismiss_update_applied: () => {
			view = { ...view, applied: null };
			return null;
		},
		set_update_settings: (args) => {
			view = { ...view, ...(args.input as Pick<UpdateView, 'channel' | 'automatic'>) };
			return view;
		},
	};
}
