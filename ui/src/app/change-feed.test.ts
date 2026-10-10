import { describe, expect, it } from 'vitest';

import { invalidations } from './change-feed';

describe('invalidations', () => {
	it('maps every change to the queries that show it, once each', () => {
		const keys = invalidations([
			{ type: 'batchItems', batchId: '7' },
			{ type: 'reports', batchId: '7' },
			{ type: 'holds' },
			{ type: 'settings' },
		]);
		expect(keys).toEqual([['series', '7'], ['history'], ['folders'], ['holds'], ['locale'], ['attention']]);
	});

	it('refreshes everything for an all change', () => {
		expect(invalidations([{ type: 'all' }])).toContainEqual([]);
	});
});
