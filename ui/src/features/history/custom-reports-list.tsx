import { ActionIcon, Button, Group, Paper, Table, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useInfiniteQuery, useMutation, useQuery } from '@tanstack/react-query';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../../entities/account/queries';
import { customReportsQuery } from '../../entities/series/queries';
import { analysisName } from '../../entities/series/status';
import { commands } from '../../shared/ipc';
import type { CustomReportRow, Language, OpenTarget } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';
import { EmptyState } from '../../shared/ui/empty-state';
import {
	DoneIcon,
	OpenIcon,
	ReportFailureIcon,
	ShowInFolderIcon,
	WaitingIcon,
	WorkingIcon,
} from '../../shared/ui/icons';
import { StatusBadge } from '../../shared/ui/status-badge';
import type { Status } from '../../shared/ui/tone';

function customStatus(row: CustomReportRow, t: TFunction): Status {
	if (row.problem) {
		return { tone: 'problem', icon: ReportFailureIcon, label: t('desktop.status.report.failed') };
	}
	if (row.state === 'nothing') {
		return { tone: 'off', icon: WaitingIcon, label: t('desktop.status.report.nothing') };
	}
	if (row.stale) {
		return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.status.report.stale') };
	}
	if (row.savedPart) {
		return { tone: 'done', icon: DoneIcon, label: t('desktop.status.report.saved') };
	}
	return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.report.creating') };
}

function Actions({ row }: { row: CustomReportRow }) {
	const { t } = useTranslation();
	const open = useMutation({ mutationFn: (target: OpenTarget) => commands.openTarget(target) });
	const partId = row.savedPart;
	if (!partId) {
		return null;
	}
	return (
		<Group gap={4} justify="flex-end" wrap="nowrap">
			<ActionIcon
				aria-label={t('desktop.series.showInFolder')}
				color="stone"
				onClick={() => open.mutate({ kind: 'revealReport', partId })}
				title={t('desktop.series.showInFolder')}
				variant="subtle"
			>
				<ShowInFolderIcon size={18} stroke={ICON_STROKE} />
			</ActionIcon>
			<Button
				leftSection={<OpenIcon size={14} stroke={ICON_STROKE} />}
				onClick={() => open.mutate({ kind: 'report', partId })}
				size="xs"
				variant="default"
			>
				{t('desktop.series.open')}
			</Button>
		</Group>
	);
}

function Row({ row, language }: { row: CustomReportRow; language: Language }) {
	const { t } = useTranslation();
	return (
		<Table.Tr>
			<Table.Td>
				<Text fw={500} size="sm" truncate>
					{row.name}
				</Text>
				<Text c="stone.7" size="xs" truncate>
					{analysisName(row.analysis, language)}
				</Text>
			</Table.Td>
			<Table.Td fz="sm" w={80}>
				{row.images}
			</Table.Td>
			<Table.Td w={200}>
				<StatusBadge status={customStatus(row, t)} />
			</Table.Td>
			<Table.Td c="stone.7" fz="sm" w={130}>
				{formatDateTime(row.createdAt)}
			</Table.Td>
			<Table.Td w={140}>
				<Actions row={row} />
			</Table.Td>
		</Table.Tr>
	);
}

export function CustomReportsList({ q }: { q: string | null }) {
	const { t } = useTranslation();
	const language = useQuery(localeQuery).data?.language ?? 'en';
	const pages = useInfiniteQuery(customReportsQuery(q));
	const rows = pages.data?.pages.flatMap((page) => page.rows) ?? [];
	if (pages.data && rows.length === 0) {
		return <EmptyState text={t('desktop.history.customEmptyText')} title={t('desktop.history.customEmptyTitle')} />;
	}
	return (
		<Paper withBorder>
			<Table highlightOnHover layout="fixed" verticalSpacing="sm">
				<Table.Thead>
					<Table.Tr>
						<Table.Th>{t('desktop.history.columns.name')}</Table.Th>
						<Table.Th w={80}>{t('desktop.history.columns.images')}</Table.Th>
						<Table.Th w={200}>{t('desktop.history.columns.status')}</Table.Th>
						<Table.Th w={130}>{t('desktop.history.columns.created')}</Table.Th>
						<Table.Th w={140} />
					</Table.Tr>
				</Table.Thead>
				<Table.Tbody>
					{rows.map((row) => (
						<Row key={row.id} language={language} row={row} />
					))}
				</Table.Tbody>
			</Table>
			{pages.hasNextPage && (
				<Group justify="center" p="sm">
					<Button loading={pages.isFetchingNextPage} onClick={() => pages.fetchNextPage()} variant="default">
						{t('desktop.history.more')}
					</Button>
				</Group>
			)}
		</Paper>
	);
}
