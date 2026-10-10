import { Button, Group, Paper, Text } from '@mantine/core';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../../entities/account/queries';
import { foldersQuery } from '../../entities/folder/queries';
import { useOrigin } from '../../entities/series/origin';
import { historyPagesQuery } from '../../entities/series/queries';
import { type HistorySearch, historyFilter, isFiltered } from '../../entities/series/search';
import type { HistoryPage, HistoryRow } from '../../shared/ipc/bindings';
import { EmptyState } from '../../shared/ui/empty-state';
import { CustomReportDialog } from './custom-report-dialog';
import { HistoryTable, type SortBy } from './history-table';

type ListProps = {
	search: HistorySearch;
	onSearch: (next: HistorySearch) => void;
	empty: { title: string; text: string };
};

function rowsOf(pages: HistoryPage[] | undefined): HistoryRow[] {
	const seen = new Set<string>();
	const all = [...(pages?.[0]?.head ?? []), ...(pages ?? []).flatMap((page) => page.rows)];
	return all.filter((row) => !seen.has(row.summary.id) && seen.add(row.summary.id));
}

function SelectionBar({
	selected,
	onClear,
	onCreate,
}: {
	selected: number;
	onClear: () => void;
	onCreate: () => void;
}) {
	const { t } = useTranslation();
	return (
		<Paper bg="jade.0" px="md" py="xs" withBorder>
			<Group justify="space-between" wrap="nowrap">
				<Text fw={500} size="sm">
					{t('desktop.history.selected', { count: selected })}
				</Text>
				<Group gap="sm" wrap="nowrap">
					<Button onClick={onClear} size="xs" variant="subtle">
						{t('desktop.history.clearSelection')}
					</Button>
					<Button onClick={onCreate} size="xs">
						{t('desktop.history.createReport')}
					</Button>
				</Group>
			</Group>
		</Paper>
	);
}

function Filtered({ onClear }: { onClear: () => void }) {
	const { t } = useTranslation();
	return (
		<EmptyState
			action={
				<Button onClick={onClear} variant="default">
					{t('desktop.history.clearFilters')}
				</Button>
			}
			text={t('desktop.history.nothingText')}
			title={t('desktop.history.nothingTitle')}
		/>
	);
}

function useSelection() {
	const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
	const toggle = (id: string) =>
		setSelected((current) => {
			const next = new Set(current);
			if (!next.delete(id)) {
				next.add(id);
			}
			return next;
		});
	return { selected, toggle, clear: () => setSelected(new Set()) };
}

export function SeriesList({ search, onSearch, empty }: ListProps) {
	const filter = historyFilter(search);
	const pages = useInfiniteQuery(historyPagesQuery(filter));
	const origin = useOrigin(useQuery(foldersQuery).data?.folders ?? []);
	const language = useQuery(localeQuery).data?.language ?? 'en';
	const selection = useSelection();
	const [dialog, setDialog] = useState<string[] | null>(null);
	const { t } = useTranslation();
	const rows = rowsOf(pages.data?.pages);
	const sort = { by: filter.sort, ascending: filter.ascending };
	const onSort = (by: SortBy) => {
		const ascending = by === sort.by ? !sort.ascending : by === 'name';
		onSearch({ ...search, sort: by, dir: ascending ? 'asc' : 'desc' });
	};
	if (pages.data && rows.length === 0) {
		const { origin: kept, ...rest } = search;
		const cleared = kept ? { origin: kept } : {};
		return isFiltered(rest) ? <Filtered onClear={() => onSearch(cleared)} /> : <EmptyState {...empty} />;
	}
	return (
		<>
			{selection.selected.size > 0 && (
				<SelectionBar
					onClear={selection.clear}
					onCreate={() => setDialog([...selection.selected])}
					selected={selection.selected.size}
				/>
			)}
			<Paper withBorder>
				<HistoryTable
					language={language}
					onSort={onSort}
					onToggle={selection.toggle}
					origin={origin}
					rows={rows}
					selected={selection.selected}
					sort={sort}
				/>
				{pages.hasNextPage && (
					<Group justify="center" p="sm">
						<Button loading={pages.isFetchingNextPage} onClick={() => pages.fetchNextPage()} variant="default">
							{t('desktop.history.more')}
						</Button>
					</Group>
				)}
			</Paper>
			{dialog && <CustomReportDialog onClose={() => setDialog(null)} series={dialog} />}
		</>
	);
}
