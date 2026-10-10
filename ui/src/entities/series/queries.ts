import { infiniteQueryOptions, queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';
import type { HistoryCursor, HistoryFilter, HistoryPage } from '../../shared/ipc/bindings';
import { NO_FILTER } from './search';

const EMPTY_PAGE_RETRIES = 10;

async function filledPage(filter: HistoryFilter, cursor: HistoryCursor | null): Promise<HistoryPage> {
	let page = await commands.listHistory(filter, cursor);
	for (let round = 0; round < EMPTY_PAGE_RETRIES && page.rows.length === 0 && page.next; round += 1) {
		const next = await commands.listHistory(filter, page.next);
		page = { ...next, head: page.head };
	}
	return page;
}

export const historyQuery = (filter: HistoryFilter = NO_FILTER) =>
	queryOptions({ queryKey: ['history', 'first', filter], queryFn: () => commands.listHistory(filter, null) });

export const historyPagesQuery = (filter: HistoryFilter) =>
	infiniteQueryOptions({
		queryKey: ['history', 'pages', filter],
		queryFn: ({ pageParam }) => filledPage(filter, pageParam),
		initialPageParam: null as HistoryCursor | null,
		getNextPageParam: (page) => page.next,
	});

export const customReportsQuery = (q: string | null) =>
	infiniteQueryOptions({
		queryKey: ['history', 'custom', q],
		queryFn: ({ pageParam }) => commands.listCustomReports(q, pageParam),
		initialPageParam: null as string | null,
		getNextPageParam: (page) => page.next,
	});

export const customDraftQuery = (series: string[]) =>
	queryOptions({
		queryKey: ['history', 'draft', series],
		queryFn: () => commands.getCustomReportDraft(series),
		staleTime: 0,
	});

export const seriesQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id], queryFn: () => commands.getBatch(id) });

export const seriesItemsQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id, 'items'], queryFn: () => commands.listBatchItems(id, null) });

export const seriesReportsQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id, 'reports'], queryFn: () => commands.listSeriesReports(id) });

export const seriesEventsQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id, 'events'], queryFn: () => commands.listBatchEvents(id, null) });
