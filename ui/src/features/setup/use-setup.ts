import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';

import { setupQuery } from '../../entities/setup/queries';
import { commands } from '../../shared/ipc';
import type { SetupInput, SetupOption, SetupView } from '../../shared/ipc/bindings';
import type { WizardStep } from './steps';

export function withOption(options: SetupOption[], option: SetupOption, on: boolean): SetupOption[] {
	const rest = options.filter((candidate) => candidate !== option);
	return on ? [...rest, option] : rest;
}

function inputOf(view: SetupView, changes: Partial<SetupInput> = {}): SetupInput {
	return {
		step: view.step,
		preset: view.preset,
		reportMode: view.report?.mode ?? 'subfolder',
		models: view.models,
		options: view.options,
		legacyTake: view.legacy?.take ?? null,
		...changes,
	};
}

export function useSetupView() {
	const client = useQueryClient();
	return (next: SetupView) => client.setQueryData(setupQuery.queryKey, next);
}

export function useSave(view: SetupView) {
	const store = useSetupView();
	const save = useMutation({
		mutationFn: (input: SetupInput) => commands.saveSetup(input),
		onSuccess: store,
	});
	return {
		saving: save.isPending,
		change: (changes: Partial<SetupInput>) => save.mutateAsync(inputOf(view, changes)),
	};
}

export function useGo(view: SetupView) {
	const navigate = useNavigate();
	const { change } = useSave(view);
	return async (step: WizardStep) => {
		if (step !== 'done') {
			await change({ step });
		}
		await navigate({ to: '/welcome/$step', params: { step } });
	};
}
