import { Alert, Box, Group, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useTranslation } from 'react-i18next';

import type { BatchSummary } from '../../shared/ipc/bindings';
import { DoneIcon, ImageProblemIcon, ReportFailureIcon } from '../../shared/ui/icons';

function useSentences(series: BatchSummary): string[] {
	const { t } = useTranslation();
	const { counts } = series;
	return [
		counts.reportsProblem > 0 ? t('desktop.series.summary.reportProblem') : '',
		counts.reportsStale > 0 ? t('desktop.series.summary.stale') : '',
		counts.problems > 0 ? t('desktop.series.summary.problems', { count: counts.problems }) : '',
	].filter((sentence) => sentence.length > 0);
}

export function SeriesSummary({ series }: { series: BatchSummary }) {
	const { t } = useTranslation();
	const sentences = useSentences(series);
	const complete =
		!series.open && series.counts.reportsTotal > 0 && series.counts.reportsSaved === series.counts.reportsTotal;
	if (sentences.length > 0) {
		const red = series.counts.reportsProblem > 0;
		const color = red ? 'red' : 'orange';
		const SummaryIcon = red ? ReportFailureIcon : ImageProblemIcon;
		return (
			<Alert bg={`${color}.0`} color={color} icon={<SummaryIcon size={20} stroke={ICON_STROKE} />}>
				{sentences.join(' ')}
			</Alert>
		);
	}
	if (!complete) {
		return null;
	}
	return (
		<Group gap={6}>
			<Box c="forest.6" lh={0}>
				<DoneIcon size={16} stroke={ICON_STROKE} />
			</Box>
			<Text c="stone.7" size="sm">
				{t('desktop.series.summary.complete')}
			</Text>
		</Group>
	);
}
