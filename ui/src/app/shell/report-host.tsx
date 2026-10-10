import { useQuery } from '@tanstack/react-query';
import { lazy, Suspense } from 'react';

import { supportQuery, useReportRequest } from '../../entities/support/queries';

const ReportDialog = lazy(() =>
	import('../../features/support/report-dialog').then((module) => ({ default: module.ReportDialog })),
);

export function ReportHost() {
	const request = useReportRequest();
	const support = useQuery({ ...supportQuery, enabled: request !== null }).data;
	if (!(request && support) || support.reporting === 'off') {
		return null;
	}
	return (
		<Suspense>
			<ReportDialog request={request} support={support} />
		</Suspense>
	);
}
