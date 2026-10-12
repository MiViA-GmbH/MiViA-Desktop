import { ActionIcon, SegmentedControl, Table, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconEye } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { hasProblem, itemStatus } from '../../entities/series/item-status';
import { analysisName } from '../../entities/series/status';
import type { AnalysisView, BatchItem, Language } from '../../shared/ipc/bindings';
import { Panel } from '../../shared/ui/panel';
import { StatusBadge } from '../../shared/ui/status-badge';

type ImagesPanelProps = { seriesId: string; items: BatchItem[]; analyses: AnalysisView[]; language: Language };

function Filter({ problems, value, onChange }: { problems: number; value: string; onChange: (next: string) => void }) {
	const { t } = useTranslation();
	const data = [
		{ value: 'all', label: t('desktop.series.filterAll') },
		{ value: 'problems', label: t('desktop.series.filterProblems', { count: problems }) },
	];
	return <SegmentedControl data={data} onChange={onChange} size="xs" value={value} />;
}

export function ImagesPanel({ seriesId, items, analyses, language }: ImagesPanelProps) {
	const { t } = useTranslation();
	const [filter, setFilter] = useState('all');
	const problems = items.filter(hasProblem);
	const shown = filter === 'problems' ? problems : items;
	return (
		<Panel
			extra={<Filter onChange={setFilter} problems={problems.length} value={filter} />}
			title={t('desktop.series.images')}
		>
			<Table layout="fixed">
				<Table.Thead>
					<Table.Tr>
						<Table.Th>{t('desktop.series.image')}</Table.Th>
						{analyses.map((analysis) => (
							<Table.Th key={analysis.key} w={200}>
								{analysisName(analysis, language)}
							</Table.Th>
						))}
						<Table.Th w={48} />
					</Table.Tr>
				</Table.Thead>
				<Table.Tbody>
					{shown.map((item) => (
						<Table.Tr key={item.id}>
							<Table.Td>
								<Text size="sm" title={item.label} truncate>
									{item.label}
								</Text>
							</Table.Td>
							{analyses.map((analysis) => (
								<Table.Td key={analysis.key}>
									<StatusBadge status={itemStatus(item, analysis.key, t)} />
								</Table.Td>
							))}
							<Table.Td>
								<ActionIcon
									aria-label={t('desktop.preview.open', { name: item.label })}
									color="stone"
									renderRoot={(props) => (
										<Link params={{ seriesId, itemId: item.id }} to="/preview/$seriesId/$itemId" {...props} />
									)}
									size="md"
									variant="subtle"
								>
									<IconEye size={16} stroke={ICON_STROKE} />
								</ActionIcon>
							</Table.Td>
						</Table.Tr>
					))}
				</Table.Tbody>
			</Table>
		</Panel>
	);
}
