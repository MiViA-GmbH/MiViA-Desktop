import { MantineProvider } from '@mantine/core';
import { v8CssVariablesResolver } from '@mivia-ai/theme';
import { QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { lazy, Suspense, useEffect } from 'react';

import { commands } from '../shared/ipc';
import { queryClient } from './query-client';
import { router } from './router';
import { desktopTheme } from './theme';

const Notifications = lazy(() =>
	import('@mantine/notifications').then((module) => ({ default: module.Notifications })),
);

function useShowWindow(): void {
	useEffect(() => {
		const paint = () => {
			requestAnimationFrame(() => {
				commands.uiReady();
			});
		};
		const onVisible = () => {
			if (document.visibilityState === 'visible') {
				paint();
			}
		};
		paint();
		document.addEventListener('visibilitychange', onVisible);
		return () => document.removeEventListener('visibilitychange', onVisible);
	}, []);
}

export function App() {
	useShowWindow();
	return (
		<MantineProvider cssVariablesResolver={v8CssVariablesResolver} forceColorScheme="light" theme={desktopTheme}>
			<Suspense>
				<Notifications position="bottom-right" />
			</Suspense>
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>
		</MantineProvider>
	);
}
