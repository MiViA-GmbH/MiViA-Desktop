import { Checkbox, Table, Text, UnstyledButton } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconArrowDown, IconArrowUp } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import type { TFunction } from 'i18next';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { analysisName, seriesStatus } from '../../entities/series/status';
import type { BatchSummary, HistoryRow, Language } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';
import { StatusBadge } from '../../shared/ui/status-badge';
import { RowActions } from './row-actions';
import classes from './sort-header.module.css';

export type SortBy = 'finished' | 'name';
type Sort = { by: SortBy; ascending: boolean };

type TableProps = {
	rows: HistoryRow[];
	selected: ReadonlySet<string>;
	onToggle: (id: string) => void;
	onToggleAll: () => void;
	sort: Sort;
	onSort: (by: SortBy) => void;
	origin: (series: BatchSummary) => string;
	language: Language;
};

function SortHeader({
	by,
	sort,
	onSort,
	children,
}: {
	by: SortBy;
	sort: Sort;
	onSort: (by: SortBy) => void;
	children: ReactNode;
}) {
	const active = sort.by === by;
	const Arrow = sort.ascending ? IconArrowUp : IconArrowDown;
	return (
		<UnstyledButton aria-pressed={active} classNames={classes} onClick={() => onSort(by)}>
			{children}
			{active && <Arrow size={14} stroke={ICON_STROKE} />}
		</UnstyledButton>
	);
}

function Analyses({ row, language }: { row: HistoryRow; language: Language }) {
	const names = row.analyses.map((analysis) => analysisName(analysis, language));
	const shown = names.slice(0, 2).join(', ');
	const more = names.length > 2 ? ` +${names.length - 2}` : '';
	return (
		<Text c="stone.7" size="sm" truncate>
			{shown + more}
		</Text>
	);
}

function secondLine(row: HistoryRow, origin: string, t: TFunction): string {
	if (!row.matched) {
		return origin;
	}
	const key = row.matched.kind === 'image' ? 'desktop.history.matchedImage' : 'desktop.history.matchedReport';
	return t(key, { name: row.matched.name });
}

function SeriesCell({ row, origin }: { row: HistoryRow; origin: string }) {
	const { t } = useTranslation();
	const { summary } = row;
	const second = secondLine(row, origin, t);
	return (
		<>
			<Text
				c="dark.7"
				fw={500}
				lineClamp={2}
				renderRoot={(props) => <Link params={{ seriesId: summary.id }} to="/series/$seriesId" {...props} />}
				size="sm"
				td="none"
			>
				{summary.label}
			</Text>
			<Text c="stone.7" size="xs" truncate>
				{second}
			</Text>
		</>
	);
}

function finished(summary: BatchSummary, since: (date: string) => string): string {
	return summary.closedAt ? formatDateTime(summary.closedAt) : since(formatDateTime(summary.openedAt));
}

function SelectAll(props: TableProps) {
	const { t } = useTranslation();
	const count = props.rows.filter((row) => props.selected.has(row.summary.id)).length;
	const all = count > 0 && count === props.rows.length;
	return (
		<Checkbox
			aria-label={t('desktop.history.selectAll')}
			checked={all}
			indeterminate={count > 0 && !all}
			onChange={props.onToggleAll}
			size="xs"
		/>
	);
}

function Row({ row, props }: { row: HistoryRow; props: TableProps }) {
	const { t } = useTranslation();
	const { summary } = row;
	return (
		<Table.Tr>
			<Table.Td w={40}>
				<Checkbox
					aria-label={t('desktop.history.selectRow', { label: summary.label })}
					checked={props.selected.has(summary.id)}
					onChange={() => props.onToggle(summary.id)}
					size="xs"
				/>
			</Table.Td>
			<Table.Td>
				<SeriesCell origin={props.origin(summary)} row={row} />
			</Table.Td>
			<Table.Td fz="sm" w={64}>
				{summary.counts.files}
			</Table.Td>
			<Table.Td visibleFrom="md" w={150}>
				<Analyses language={props.language} row={row} />
			</Table.Td>
			<Table.Td w={176}>
				<StatusBadge status={seriesStatus(summary, t)} />
			</Table.Td>
			<Table.Td c="stone.7" fz="sm" w={136}>
				{finished(summary, (date) => t('desktop.history.since', { date }))}
			</Table.Td>
			<Table.Td w={76}>
				<RowActions row={row} />
			</Table.Td>
		</Table.Tr>
	);
}

export function HistoryTable(props: TableProps) {
	const { t } = useTranslation();
	return (
		<Table highlightOnHover layout="fixed" verticalSpacing="sm">
			<Table.Thead>
				<Table.Tr>
					<Table.Th w={40}>
						<SelectAll {...props} />
					</Table.Th>
					<Table.Th>
						<SortHeader by="name" onSort={props.onSort} sort={props.sort}>
							{t('desktop.history.columns.series')}
						</SortHeader>
					</Table.Th>
					<Table.Th w={64}>{t('desktop.history.columns.images')}</Table.Th>
					<Table.Th visibleFrom="md" w={150}>
						{t('desktop.history.columns.analyses')}
					</Table.Th>
					<Table.Th w={176}>{t('desktop.history.columns.status')}</Table.Th>
					<Table.Th w={136}>
						<SortHeader by="finished" onSort={props.onSort} sort={props.sort}>
							{t('desktop.history.columns.finished')}
						</SortHeader>
					</Table.Th>
					<Table.Th aria-label={t('desktop.history.columns.reports')} w={76} />
				</Table.Tr>
			</Table.Thead>
			<Table.Tbody>
				{props.rows.map((row) => (
					<Row key={row.summary.id} props={props} row={row} />
				))}
			</Table.Tbody>
		</Table>
	);
}
