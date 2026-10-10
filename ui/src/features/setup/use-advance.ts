import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { setupQuery } from '../../entities/setup/queries';
import { commands, isAppError } from '../../shared/ipc';
import type { DoneView, SetupStep, SetupView } from '../../shared/ipc/bindings';
import { isLast, nextStep } from './steps';
import { useGo } from './use-setup';

export const DONE_KEY = ['setupDone'] as const;
export const ERROR_KEY = ['setupError'] as const;
export const IMPORT_KEY = ['legacyImport'] as const;

export type SetupError = { step: SetupStep; message: string };

const FAILURES = {
	keyMissing: { step: 'key', message: 'desktop.wizard.key.rejected' },
	keyRejected: { step: 'key', message: 'desktop.wizard.key.rejected' },
	folderNotFound: { step: 'folder', message: 'desktop.wizard.folder.notFound' },
	notAFolder: { step: 'folder', message: 'desktop.wizard.folder.notFound' },
	folderOverlaps: { step: 'folder', message: 'desktop.addFolder.page.overlaps' },
	reportFolderDenied: { step: 'series', message: 'desktop.addFolder.page.denied' },
} as const;

function isFailure(code: string | null | undefined): code is keyof typeof FAILURES {
	return typeof code === 'string' && code in FAILURES;
}

function failure(error: unknown) {
	const code = isAppError(error) ? error.code : null;
	return isFailure(code) ? FAILURES[code] : null;
}

function useComplete(view: SetupView) {
	const client = useQueryClient();
	const navigate = useNavigate();
	const { t } = useTranslation();
	return useMutation({
		mutationFn: () => commands.completeSetup(),
		meta: { inline: true },
		onSuccess: async (done: DoneView) => {
			client.setQueryData(DONE_KEY, done);
			client.setQueryData(ERROR_KEY, null);
			client.setQueryData(setupQuery.queryKey, { ...view, completed: true });
			await navigate({ to: '/welcome/$step', params: { step: 'done' } });
		},
		onError: async (error) => {
			const found = failure(error);
			const shown: SetupError = found
				? { step: found.step, message: t(found.message) }
				: { step: view.step, message: t('desktop.errors.actionFailed') };
			client.setQueryData(ERROR_KEY, shown);
			await navigate({ to: '/welcome/$step', params: { step: shown.step } });
		},
	});
}

export function useAdvance(view: SetupView, step: SetupStep) {
	const { t } = useTranslation();
	const go = useGo(view);
	const complete = useComplete(view);
	const last = isLast(view, step);
	return {
		label: last ? t('desktop.wizard.finish.button') : t('desktop.wizard.common.next'),
		loading: complete.isPending,
		advance: () => (last ? complete.mutate() : go(nextStep(view, step))),
	};
}
