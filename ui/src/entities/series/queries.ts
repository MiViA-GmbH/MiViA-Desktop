import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const historyQuery = queryOptions({ queryKey: ['history'], queryFn: () => commands.listHistory(null) });

export const seriesQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id], queryFn: () => commands.getBatch(id) });

export const seriesItemsQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id, 'items'], queryFn: () => commands.listBatchItems(id, null) });

export const seriesReportsQuery = (id: string) =>
	queryOptions({ queryKey: ['series', id, 'reports'], queryFn: () => commands.listSeriesReports(id) });
