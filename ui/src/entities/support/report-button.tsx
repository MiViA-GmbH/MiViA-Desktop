import { Button } from '@mantine/core';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { useReporting } from './hooks';
import { requestReport } from './queries';

export function ReportButton() {
	const { t } = useTranslation();
	const client = useQueryClient();
	const reporting = useReporting();
	if (!reporting) {
		return null;
	}
	return (
		<Button
			flex="none"
			onClick={() => requestReport(client, { trigger: 'manual', signature: null })}
			size="xs"
			variant="default"
		>
			{t('desktop.update.report')}
		</Button>
	);
}
