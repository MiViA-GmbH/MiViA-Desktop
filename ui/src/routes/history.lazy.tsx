import '../features/history/history.css';

import { Button, Group, SegmentedControl, TextInput } from '@mantine/core';
import { useDebouncedCallback } from '@mantine/hooks';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconDownload, IconSearch } from '@tabler/icons-react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute, useNavigate, useSearch } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { historyQuery } from '../entities/series/queries';
import { type HistorySearch, historyFilter } from '../entities/series/search';
import { CustomReportsList } from '../features/history/custom-reports-list';
import { HistoryToolbar } from '../features/history/history-toolbar';
import { SeriesList } from '../features/history/series-list';
import { useExport } from '../features/history/use-export';
import { commands } from '../shared/ipc';
import { AddImagesIcon } from '../shared/ui/icons';
import { PageHeader } from '../shared/ui/page-header';

const SEARCH_DELAY = 250;

function useAnySeries(search: HistorySearch): boolean {
	const rows = useQuery(historyQuery(historyFilter(search))).data?.rows;
	return rows === undefined || rows.length > 0;
}

function ExportList({ search }: { search: HistorySearch }) {
	const { t } = useTranslation();
	const exporter = useExport();
	return (
		<Button
			leftSection={<IconDownload size={16} stroke={ICON_STROKE} />}
			loading={exporter.isPending}
			onClick={() => exporter.mutate({ filter: historyFilter(search), series: null })}
			variant="default"
		>
			{t('desktop.history.export')}
		</Button>
	);
}

function CustomSearch({ search, onSearch }: { search: HistorySearch; onSearch: (next: HistorySearch) => void }) {
	const { t } = useTranslation();
	const commit = useDebouncedCallback(
		(value: string) => onSearch({ ...search, q: value.trim() || undefined }),
		SEARCH_DELAY,
	);
	return (
		<TextInput
			aria-label={t('desktop.history.searchCustom')}
			defaultValue={search.q ?? ''}
			leftSection={<IconSearch size={16} stroke={ICON_STROKE} />}
			maw={420}
			onChange={(event) => commit(event.currentTarget.value)}
			placeholder={t('desktop.history.searchCustom')}
		/>
	);
}

function useHistorySearch(from: '/app/history' | '/app/single-images') {
	const search = useSearch({ from });
	const navigate = useNavigate({ from: from === '/app/history' ? '/history' : '/single-images' });
	return { search, onSearch: (next: HistorySearch) => navigate({ search: next, replace: true }) };
}

function History() {
	const { t } = useTranslation();
	const { search, onSearch } = useHistorySearch('/app/history');
	const custom = search.view === 'custom';
	const any = useAnySeries({});
	const views = [
		{ value: 'series', label: t('desktop.history.viewSeries') },
		{ value: 'custom', label: t('desktop.history.viewCustom') },
	];
	const empty = { title: t('desktop.history.emptyTitle'), text: t('desktop.history.emptyText') };
	const actions = (
		<Group gap="sm" wrap="nowrap">
			<SegmentedControl
				data={views}
				onChange={(value) => onSearch(value === 'custom' ? { view: 'custom' } : {})}
				value={custom ? 'custom' : 'series'}
			/>
			<ExportList search={search} />
		</Group>
	);
	return (
		<>
			<PageHeader actions={actions} meta={t('desktop.history.meta')} title={t('desktop.nav.history')} />
			{custom && <CustomSearch onSearch={onSearch} search={search} />}
			{custom && <CustomReportsList q={search.q ?? null} />}
			{!custom && any && <HistoryToolbar onChange={onSearch} search={search} />}
			{!custom && <SeriesList empty={empty} onSearch={onSearch} search={search} />}
		</>
	);
}

function AddImages() {
	const { t } = useTranslation();
	const add = useMutation({ mutationFn: () => commands.addImages() });
	return (
		<Button
			leftSection={<AddImagesIcon size={16} stroke={ICON_STROKE} />}
			loading={add.isPending}
			onClick={() => add.mutate()}
		>
			{t('desktop.singleImages.add')}
		</Button>
	);
}

function SingleImages() {
	const { t } = useTranslation();
	const { search, onSearch } = useHistorySearch('/app/single-images');
	const manual: HistorySearch = { ...search, origin: 'manual' };
	const any = useAnySeries({ origin: 'manual' });
	const empty = { title: t('desktop.singleImages.emptyTitle'), text: t('desktop.singleImages.emptyText') };
	return (
		<>
			<PageHeader actions={<AddImages />} meta={t('desktop.singleImages.meta')} title={t('desktop.nav.singleImages')} />
			{any && <HistoryToolbar manualOnly onChange={onSearch} search={manual} />}
			<SeriesList empty={empty} onSearch={onSearch} search={manual} />
		</>
	);
}

export const HistoryRoute = createLazyRoute('/app/history')({ component: History });
export const SingleImagesRoute = createLazyRoute('/app/single-images')({ component: SingleImages });
