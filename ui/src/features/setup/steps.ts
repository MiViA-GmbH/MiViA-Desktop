import type { SetupOption, SetupStep, SetupView } from '../../shared/ipc/bindings';

export type WizardStep = SetupStep | 'done';

const ORDER: SetupStep[] = ['welcome', 'legacy', 'key', 'folder', 'series', 'models', 'finish'];
const FOLDER_STEPS: SetupStep[] = ['series', 'models'];

export function has(view: Pick<SetupView, 'options'>, option: SetupOption): boolean {
	return view.options.includes(option);
}

function finishHidden(view: SetupView): boolean {
	const { autostart, explorer, telemetry } = view.locks;
	return autostart !== null && explorer !== null && telemetry !== null;
}

export function shownSteps(view: SetupView): SetupStep[] {
	return ORDER.filter((step) => {
		if (step === 'legacy') {
			return view.legacy !== null && view.locks.legacySearch;
		}
		return step !== 'finish' || !finishHidden(view);
	});
}

export function isSkipped(view: SetupView, step: SetupStep): boolean {
	return has(view, 'skipFolder') && FOLDER_STEPS.includes(step);
}

export function countedSteps(view: SetupView): SetupStep[] {
	return shownSteps(view).filter((step) => step !== 'welcome' && !isSkipped(view, step));
}

export function stepperSteps(view: SetupView): SetupStep[] {
	return shownSteps(view).filter((step) => step !== 'welcome');
}

export function nextStep(view: SetupView, step: SetupStep): SetupStep | 'done' {
	const order = shownSteps(view).filter((candidate) => !isSkipped(view, candidate));
	const index = order.indexOf(step);
	return order[index + 1] ?? 'done';
}

export function previousStep(view: SetupView, step: SetupStep): SetupStep {
	const order = shownSteps(view).filter((candidate) => !isSkipped(view, candidate));
	return order[Math.max(order.indexOf(step) - 1, 0)] ?? 'welcome';
}

export function isLast(view: SetupView, step: SetupStep): boolean {
	return nextStep(view, step) === 'done';
}

export function isStep(value: string): value is WizardStep {
	return value === 'done' || ORDER.some((step) => step === value);
}
