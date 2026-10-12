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
	required: null,
};

const SCENARIOS: Record<string, Partial<UpdateView>> = {
	'update-ready': { state: 'ready', version: '2.1.0', failedInstall: '2.1.0' },
	'update-unsigned': { required: { min: '2.1.0', signed: false } },
	'update-locked': { automatic: false, policy: 'off' },
	'update-available': { state: 'available', version: '2.1.0' },
	'update-downloading': { state: 'downloading', version: '2.1.0' },
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
		set_update_settings: (args) => {
			view = { ...view, ...(args.input as Pick<UpdateView, 'channel' | 'automatic'>) };
			return view;
		},
	};
}
