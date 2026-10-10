import { Button, Table, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { Link, useNavigate } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { seriesStatus } from '../../entities/series/status';
import type { BatchSummary } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';
import { ReportIcon } from '../../shared/ui/icons';
import { StatusBadge } from '../../shared/ui/status-badge';

type SeriesTableProps = { rows: BatchSummary[]; origin: (series: BatchSummary) => string };

function finished(series: BatchSummary): string {
	return series.closedAt ? formatDateTime(series.closedAt) : '';
}

export function SeriesTable({ rows, origin }: SeriesTableProps) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const open = (series: BatchSummary) => navigate({ to: '/series/$seriesId', params: { seriesId: series.id } });
	return (
		<Table highlightOnHover layout="fixed">
			<Table.Tbody>
				{rows.map((series) => (
					<Table.Tr key={series.id}>
						<Table.Td>
							<Text
								c="dark.7"
								fw={500}
								renderRoot={(props) => <Link params={{ seriesId: series.id }} to="/series/$seriesId" {...props} />}
								size="sm"
								td="none"
								truncate
							>
								{series.label}
							</Text>
							<Text c="stone.7" size="xs" truncate>
								{origin(series)}
							</Text>
						</Table.Td>
						<Table.Td c="stone.7" fz="sm" w={130}>
							{finished(series)}
						</Table.Td>
						<Table.Td w={190}>
							<StatusBadge status={seriesStatus(series, t)} />
						</Table.Td>
						<Table.Td w={150}>
							{series.counts.reportsSaved > 0 && (
								<Button
									leftSection={<ReportIcon size={14} stroke={ICON_STROKE} />}
									onClick={() => open(series)}
									size="compact-xs"
									variant="subtle"
								>
									{t('desktop.series.reportsCount', { count: series.counts.reportsSaved })}
								</Button>
							)}
						</Table.Td>
					</Table.Tr>
				))}
			</Table.Tbody>
		</Table>
	);
}
