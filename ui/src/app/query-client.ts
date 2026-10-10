import { MutationCache, QueryClient } from '@tanstack/react-query';

export const queryClient: QueryClient = new QueryClient({
	defaultOptions: {
		queries: { networkMode: 'always', retry: false, staleTime: Number.POSITIVE_INFINITY, refetchOnWindowFocus: false },
		mutations: { networkMode: 'always', retry: false },
	},
	mutationCache: new MutationCache({
		onError: (error, _variables, _context, mutation) => {
			if (mutation.meta?.inline !== true) {
				import('../entities/support/toasts').then(({ showFailure }) => showFailure(error, queryClient));
			}
		},
	}),
});
