import { type QueryClient, queryOptions, useQuery } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';
import type { ReportTrigger } from '../../shared/ipc/bindings';

export const SUPPORT = ['support'] as const;

export const supportQuery = queryOptions({ queryKey: SUPPORT, queryFn: () => commands.getSupport() });

export const pendingReportsQuery = queryOptions({
	queryKey: [...SUPPORT, 'pending'],
	queryFn: () => commands.listPendingReports(),
	staleTime: 0,
});

export function contactText(contact: string): string {
	return contact.replace(/^mailto:/i, '');
}

export type ReportRequest = { trigger: ReportTrigger; signature: string | null };

const REPORT_REQUEST = ['reportRequest'] as const;

export function requestReport(client: QueryClient, request: ReportRequest | null): void {
	client.setQueryData(REPORT_REQUEST, request);
}

export function useReportRequest(): ReportRequest | null {
	return useQuery<ReportRequest | null>({ queryKey: REPORT_REQUEST, queryFn: () => null, enabled: false }).data ?? null;
}

export function useReporting(): boolean {
	const view = useQuery(supportQuery).data;
	return view !== undefined && view.reporting !== 'off';
}
