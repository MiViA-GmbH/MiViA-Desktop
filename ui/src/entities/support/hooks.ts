import { useQuery } from '@tanstack/react-query';

import { REPORT_REQUEST, type ReportRequest, supportQuery } from './queries';

export function useReportRequest(): ReportRequest | null {
	return useQuery<ReportRequest | null>({ queryKey: REPORT_REQUEST, queryFn: () => null, enabled: false }).data ?? null;
}

export function useReporting(): boolean {
	const view = useQuery(supportQuery).data;
	return view !== undefined && view.reporting !== 'off';
}
