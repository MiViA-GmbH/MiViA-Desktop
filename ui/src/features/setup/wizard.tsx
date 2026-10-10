import { useQuery } from '@tanstack/react-query';
import { Navigate } from '@tanstack/react-router';

import type { DoneView, SetupView } from '../../shared/ipc/bindings';
import { DoneStep } from './done-step';
import { FinishStep } from './finish-step';
import { FolderStep } from './folder-step';
import { KeyStep } from './key-step';
import { LegacyStep } from './legacy-step';
import { ModelsStep } from './models-step';
import { SeriesStep } from './series-step';
import { isStep, shownSteps, type WizardStep } from './steps';
import { DONE_KEY } from './use-advance';
import { WelcomeStep } from './welcome-step';

function Done({ view }: { view: SetupView }) {
	const done = useQuery<DoneView | null>({ queryKey: DONE_KEY, queryFn: () => null, enabled: false }).data;
	return done ? <DoneStep done={done} view={view} /> : <Navigate to="/" />;
}

function Step({ view, step }: { view: SetupView; step: WizardStep }) {
	switch (step) {
		case 'welcome':
			return <WelcomeStep view={view} />;
		case 'legacy':
			return view.legacy ? <LegacyStep legacy={view.legacy} view={view} /> : <KeyStep view={view} />;
		case 'key':
			return <KeyStep view={view} />;
		case 'folder':
			return <FolderStep view={view} />;
		case 'series':
			return <SeriesStep view={view} />;
		case 'models':
			return <ModelsStep view={view} />;
		case 'finish':
			return <FinishStep view={view} />;
		case 'done':
			return <Done view={view} />;
		default:
			return step satisfies never;
	}
}

export function Wizard({ view, step }: { view: SetupView; step: string }) {
	if (!isStep(step)) {
		return <Navigate params={{ step: view.step }} to="/welcome/$step" />;
	}
	if (view.completed && step !== 'done') {
		return <Navigate to="/" />;
	}
	const allowed = step === 'done' || shownSteps(view).includes(step);
	return allowed ? <Step step={step} view={view} /> : <Navigate params={{ step: 'welcome' }} to="/welcome/$step" />;
}
