import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const setupQuery = queryOptions({ queryKey: ['setup'], queryFn: () => commands.getSetup() });

export const catalogueQuery = queryOptions({ queryKey: ['catalogue'], queryFn: () => commands.getCatalogue() });

export const integrationQuery = queryOptions({
	queryKey: ['integration'],
	queryFn: () => commands.getIntegration(),
});

export const QUIT_REQUESTED = ['quitRequested'] as const;
