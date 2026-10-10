import { describe, expect, it } from 'vitest';

import { historyFilter, isFiltered, parseHistorySearch } from './search';

describe('history search params', () => {
	it('keeps only known values and drops the rest', () => {
		const parsed = parseHistorySearch({
			q: 'Probe',
			folder: '3',
			origin: 'web',
			status: 'out-of-date',
			from: '2026-10-01',
			to: 'yesterday',
			sort: 'name',
			dir: 'up',
			view: 'custom',
			extra: 'x',
		});
		expect(parsed).toEqual({
			q: 'Probe',
			folder: '3',
			status: 'out-of-date',
			from: '2026-10-01',
			sort: 'name',
			view: 'custom',
		});
	});

	it('turns params into the filter the store reads', () => {
		const filter = historyFilter({ origin: 'manual', status: 'problems', from: '2026-10-01', to: '2026-10-02' });
		expect(filter.manual).toBe(true);
		expect(filter.status).toBe('problems');
		expect(filter.sort).toBe('finished');
		expect(filter.ascending).toBe(false);
		expect(new Date(filter.from ?? '').getDate()).toBe(1);
		expect(new Date(filter.to ?? '').getDate()).toBe(2);
		expect(new Date(filter.to ?? '').getHours()).toBe(23);
	});

	it('sorts names upwards unless asked otherwise', () => {
		expect(historyFilter({ sort: 'name' }).ascending).toBe(true);
		expect(historyFilter({ sort: 'name', dir: 'desc' }).ascending).toBe(false);
		expect(historyFilter({ dir: 'asc' }).ascending).toBe(true);
	});

	it('treats sorting and the view as no filter', () => {
		expect(isFiltered({ sort: 'name', view: 'custom' })).toBe(false);
		expect(isFiltered({ analysis: 'grain' })).toBe(true);
	});
});
