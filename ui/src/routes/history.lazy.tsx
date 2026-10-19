import '../features/history/history.css';

import { Button, Group, Stack, Tabs, TextInput } from '@mantine/core';
import { useDebouncedCallback } from '@mantine/hooks';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconDownload, IconSearch } from '@tabler/icons-react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute, useNavigate, useSearch } from '@tanstack/react-router';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { historyQuery, seriesIds } from '../entities/series/queries';
import { type HistorySearch, historyFilter } from '../entities/series/search';
import { CustomReportDialog } from '../features/history/custom-report-dialog';
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
	const [series, setSeries] = useState<string[] | null>(null);
	const collect = useMutation({ mutationFn: () => seriesIds(historyFilter(search)), onSuccess: setSeries });
	return (
		<Group gap="sm" wrap="nowrap">
			<Button
				leftSection={<IconDownload size={16} stroke={ICON_STROKE} />}
				loading={collect.isPending}
				onClick={() => collect.mutate()}
				variant="default"
			>
				{t('desktop.history.export')}
			</Button>
			<Button
				loading={exporter.isPending}
				onClick={() => exporter.mutate({ filter: historyFilter(search), series: null })}
				variant="default"
			>
				{t('desktop.history.exportRecord')}
			</Button>
			{series && <CustomReportDialog csvOnly onClose={() => setSeries(null)} series={series} />}
		</Group>
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
	const empty = { title: t('desktop.history.emptyTitle'), text: t('desktop.history.emptyText') };
	return (
		<>
			<PageHeader
				actions={<ExportList search={search} />}
				meta={t('desktop.history.meta')}
				title={t('desktop.nav.history')}
			/>
			<Tabs
				keepMounted={false}
				onChange={(value) => onSearch(value === 'custom' ? { view: 'custom' } : {})}
				value={custom ? 'custom' : 'series'}
			>
				<Tabs.List mb="lg">
					<Tabs.Tab value="series">{t('desktop.history.viewSeries')}</Tabs.Tab>
					<Tabs.Tab value="custom">{t('desktop.history.viewCustom')}</Tabs.Tab>
				</Tabs.List>
				<Tabs.Panel value="series">
					<Stack gap="lg">
						{any && <HistoryToolbar onChange={onSearch} search={search} />}
						<SeriesList empty={empty} onSearch={onSearch} search={search} />
					</Stack>
				</Tabs.Panel>
				<Tabs.Panel value="custom">
					<Stack gap="lg">
						<CustomSearch onSearch={onSearch} search={search} />
						<CustomReportsList q={search.q ?? null} />
					</Stack>
				</Tabs.Panel>
			</Tabs>
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
