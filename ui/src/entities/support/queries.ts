import { type QueryClient, queryOptions } from '@tanstack/react-query';

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

export const REPORT_REQUEST = ['reportRequest'] as const;

export function requestReport(client: QueryClient, request: ReportRequest | null): void {
	client.setQueryData(REPORT_REQUEST, request);
}
