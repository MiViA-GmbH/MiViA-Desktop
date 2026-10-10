import type { QueryClient } from '@tanstack/react-query';

import { events } from '../shared/ipc';
import type { Change } from '../shared/ipc/bindings';

function keys(change: Change): string[][] {
	switch (change.type) {
		case 'batchSummary':
		case 'batchItems':
		case 'reports':
			return [['series', change.batchId], ['history'], ['folders']];
		case 'folder':
		case 'history':
			return [['folders'], ['history']];
		case 'needsAttention':
			return [['attention']];
		case 'holds':
			return [['holds']];
		case 'settings':
			return [['locale']];
		case 'catalogue':
			return [['series']];
		case 'all':
			return [[]];
		default:
			return change satisfies never;
	}
}

export function invalidations(changes: Change[]): string[][] {
	const all = changes.flatMap(keys).concat([['attention']]);
	const unique = new Map(all.map((key) => [key.join('/'), key]));
	return [...unique.values()];
}

export async function followChanges(client: QueryClient): Promise<() => void> {
	let last: string | undefined;
	return events.storeChangedEvent.listen(({ payload }) => {
		const gap = last !== undefined && payload.fromRevision !== last;
		last = payload.toRevision;
		const queryKeys = gap ? [[]] : invalidations(payload.changes);
		for (const queryKey of queryKeys) {
			client.invalidateQueries({ queryKey });
		}
	});
}
