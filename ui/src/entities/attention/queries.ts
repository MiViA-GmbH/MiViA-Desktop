import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const attentionQuery = queryOptions({
	queryKey: ['attention'],
	queryFn: () => commands.listNeedsAttention(),
});
