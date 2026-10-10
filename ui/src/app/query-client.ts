import { MutationCache, QueryClient } from '@tanstack/react-query';
import i18n from 'i18next';

import { isAppError } from '../shared/ipc';

const MESSAGES = {
	reportChanged: 'desktop.errors.reportChanged',
	reportMissing: 'desktop.errors.reportMissing',
} as const;

function isKnown(code: string | null | undefined): code is keyof typeof MESSAGES {
	return code !== null && code !== undefined && code in MESSAGES;
}

function showFailure(error: unknown): void {
	const code = isAppError(error) ? error.code : null;
	const message = i18n.t(isKnown(code) ? MESSAGES[code] : 'desktop.errors.actionFailed');
	import('@mantine/notifications').then(({ notifications }) =>
		notifications.show({ color: 'red', message, role: 'alert' }),
	);
}

export const queryClient = new QueryClient({
	defaultOptions: {
		queries: { networkMode: 'always', retry: false, staleTime: Number.POSITIVE_INFINITY, refetchOnWindowFocus: false },
		mutations: { networkMode: 'always', retry: false },
	},
	mutationCache: new MutationCache({
		onError: (error, _variables, _context, mutation) => {
			if (mutation.meta?.inline !== true) {
				showFailure(error);
			}
		},
	}),
});
