import { queryOptions } from '@tanstack/react-query';

import { commands, isAppError } from '../../shared/ipc';
import type { UpdateView } from '../../shared/ipc/bindings';

export const UPDATE = ['update'] as const;

const BUSY: UpdateView['state'][] = ['checking', 'downloading'];

export function pollInterval(view: UpdateView | null | undefined): number | false {
	if (view === null || (view !== undefined && BUSY.includes(view.state))) {
		return 2000;
	}
	return false;
}

async function getUpdate(): Promise<UpdateView | null> {
	try {
		return await commands.getUpdate();
	} catch (error) {
		if (isAppError(error) && error.code === 'updatesNotReady') {
			return null;
		}
		throw error;
	}
}

export const updateQuery = queryOptions({
	queryKey: UPDATE,
	queryFn: getUpdate,
	refetchInterval: (query) => pollInterval(query.state.data),
});
