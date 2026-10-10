import type { UpdateView } from '../../shared/ipc/bindings';

function updateView(scenario: string): UpdateView {
	const view: UpdateView = {
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
	if (scenario === 'update-ready') {
		return { ...view, state: 'ready', version: '2.1.0', failedInstall: '2.0.1' };
	}
	if (scenario === 'update-unsigned') {
		return { ...view, required: { min: '2.1.0', signed: false } };
	}
	if (scenario === 'update-locked') {
		return { ...view, automatic: false, policy: 'off' };
	}
	return view;
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
