import { Group, Paper, Progress, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { seriesStatus } from '../../entities/series/status';
import type { FolderView } from '../../shared/ipc/bindings';
import { StatusBadge } from '../../shared/ui/status-badge';
import { FinishSeriesButton } from './folder-actions';

export function OpenSeriesCard({ folder }: { folder: FolderView }) {
	const { t } = useTranslation();
	const series = folder.openSeries;
	if (!series) {
		return null;
	}
	const { files, done } = series.counts;
	return (
		<Paper p="md" withBorder>
			<Group justify="space-between" wrap="nowrap">
				<Stack flex={1} gap={6} miw={0}>
					<Group gap="xs" wrap="nowrap">
						<StatusBadge status={seriesStatus(series, t)} />
						<Text fw={600} truncate>
							{series.label}
						</Text>
					</Group>
					<Group gap="sm" wrap="nowrap">
						<Progress aria-hidden flex={1} maw={360} size="md" value={files > 0 ? (done / files) * 100 : 0} />
						<Text c="stone.7" size="sm">
							{t('desktop.folder.doneOf', { done, files })}
						</Text>
					</Group>
					<Text c="stone.7" size="xs">
						{t('desktop.folder.quietHint', { minutes: Math.max(1, Math.round(folder.quietSeconds / 60)) })}
					</Text>
				</Stack>
				<FinishSeriesButton seriesId={series.id} />
			</Group>
		</Paper>
	);
}
