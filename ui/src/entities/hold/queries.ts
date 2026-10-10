import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const networkQuery = (since: string) =>
	queryOptions({ queryKey: ['network', since], queryFn: () => commands.getNetwork() });

export const holdsQuery = queryOptions({ queryKey: ['holds'], queryFn: () => commands.listHolds() });
