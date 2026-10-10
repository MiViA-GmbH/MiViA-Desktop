import type { HistoryFilter, HistoryStatus } from '../../shared/ipc/bindings';

export type StatusParam = 'problems' | 'out-of-date' | 'active';

export type HistorySearch = {
	q?: string | undefined;
	folder?: string | undefined;
	origin?: 'manual' | undefined;
	analysis?: string | undefined;
	status?: StatusParam | undefined;
	from?: string | undefined;
	to?: string | undefined;
	sort?: 'finished' | 'name' | undefined;
	dir?: 'asc' | 'desc' | undefined;
	view?: 'custom' | undefined;
};

const STATUS: Record<StatusParam, HistoryStatus> = {
	problems: 'problems',
	'out-of-date': 'outOfDate',
	active: 'active',
};

const DATE = /^\d{4}-\d{2}-\d{2}$/;

function text(value: unknown): string | undefined {
	const found = typeof value === 'number' ? String(value) : value;
	return typeof found === 'string' && found.length > 0 ? found : undefined;
}

function oneOf<T extends string>(value: unknown, allowed: readonly T[]): T | undefined {
	return allowed.find((entry) => entry === value);
}

function date(value: unknown): string | undefined {
	const found = text(value);
	return found && DATE.test(found) ? found : undefined;
}

function compact(search: HistorySearch): HistorySearch {
	return Object.fromEntries(Object.entries(search).filter(([, value]) => value !== undefined)) as HistorySearch;
}

export function parseHistorySearch(raw: Record<string, unknown>): HistorySearch {
	return compact({
		q: text(raw.q),
		folder: text(raw.folder),
		origin: oneOf(raw.origin, ['manual'] as const),
		analysis: text(raw.analysis),
		status: oneOf(raw.status, ['problems', 'out-of-date', 'active'] as const),
		from: date(raw.from),
		to: date(raw.to),
		sort: oneOf(raw.sort, ['finished', 'name'] as const),
		dir: oneOf(raw.dir, ['asc', 'desc'] as const),
		view: oneOf(raw.view, ['custom'] as const),
	});
}

function dayStart(day: string | undefined): string | null {
	return day ? new Date(`${day}T00:00:00`).toISOString() : null;
}

function dayEnd(day: string | undefined): string | null {
	return day ? new Date(`${day}T23:59:59.999`).toISOString() : null;
}

export function historyFilter(search: HistorySearch): HistoryFilter {
	const sort = search.sort ?? 'finished';
	const descending = search.dir ? search.dir === 'desc' : sort === 'finished';
	return {
		q: search.q ?? null,
		folder: search.folder ?? null,
		manual: search.origin === 'manual',
		analysis: search.analysis ?? null,
		status: search.status ? STATUS[search.status] : null,
		from: dayStart(search.from),
		to: dayEnd(search.to),
		sort,
		ascending: !descending,
	};
}

export function isFiltered(search: HistorySearch): boolean {
	const keys: (keyof HistorySearch)[] = ['q', 'folder', 'origin', 'analysis', 'status', 'from', 'to'];
	return keys.some((key) => search[key] !== undefined);
}

export const NO_FILTER: HistoryFilter = historyFilter({});
