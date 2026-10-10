import { Chip, Group, Table, Text } from '@mantine/core';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { hasProblem, itemStatus } from '../../entities/series/item-status';
import { analysisName } from '../../entities/series/status';
import type { AnalysisView, BatchItem, Language } from '../../shared/ipc/bindings';
import { Panel } from '../../shared/ui/panel';
import { StatusBadge } from '../../shared/ui/status-badge';

type ImagesPanelProps = { items: BatchItem[]; analyses: AnalysisView[]; language: Language };

function Filter({ problems, value, onChange }: { problems: number; value: string; onChange: (next: string) => void }) {
	const { t } = useTranslation();
	return (
		<Chip.Group onChange={onChange} value={value}>
			<Group gap="xs">
				<Chip size="xs" value="all">
					{t('desktop.series.filterAll')}
				</Chip>
				<Chip size="xs" value="problems">
					{t('desktop.series.filterProblems', { count: problems })}
				</Chip>
			</Group>
		</Chip.Group>
	);
}

export function ImagesPanel({ items, analyses, language }: ImagesPanelProps) {
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
					</Table.Tr>
				</Table.Thead>
				<Table.Tbody>
					{shown.map((item) => (
						<Table.Tr key={item.id}>
							<Table.Td>
								<Text size="sm" truncate>
									{item.label}
								</Text>
							</Table.Td>
							{analyses.map((analysis) => (
								<Table.Td key={analysis.key}>
									<StatusBadge status={itemStatus(item, analysis.key, t)} />
								</Table.Td>
							))}
						</Table.Tr>
					))}
				</Table.Tbody>
			</Table>
		</Panel>
	);
}
