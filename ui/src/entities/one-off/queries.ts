import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const oneOffQuery = queryOptions({ queryKey: ['oneOff'], queryFn: () => commands.getOneOff() });

export const remainingUploadsQuery = queryOptions({
	queryKey: ['remainingUploads'],
	queryFn: () => commands.getRemainingUploads(),
	staleTime: 0,
});

export const DROP_HOVER = ['dropHover'] as const;
