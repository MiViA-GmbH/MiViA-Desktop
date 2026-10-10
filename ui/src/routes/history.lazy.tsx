import { Paper } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { createLazyRoute } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { foldersQuery } from '../entities/folder/queries';
import { useOrigin } from '../entities/series/origin';
import { historyQuery } from '../entities/series/queries';
import { SeriesTable } from '../features/series/series-table';
import type { BatchSummary } from '../shared/ipc/bindings';
import { EmptyState } from '../shared/ui/empty-state';
import { PageHeader } from '../shared/ui/page-header';

type ListProps = {
	title: string;
	meta: string;
	empty: { title: string; text: string };
	keep: (row: BatchSummary) => boolean;
};

function SeriesList({ title, meta, empty, keep }: ListProps) {
	const page = useQuery(historyQuery).data;
	const origin = useOrigin(useQuery(foldersQuery).data?.folders ?? []);
	const rows = [...(page?.head ?? []), ...(page?.rows ?? [])].filter(keep);
	return (
		<>
			<PageHeader meta={meta} title={title} />
			{page && rows.length === 0 && <EmptyState text={empty.text} title={empty.title} />}
			{rows.length > 0 && (
				<Paper withBorder>
					<SeriesTable origin={origin} rows={rows} />
				</Paper>
			)}
		</>
	);
}

function History() {
	const { t } = useTranslation();
	return (
		<SeriesList
			empty={{ title: t('desktop.history.emptyTitle'), text: t('desktop.history.emptyText') }}
			keep={() => true}
			meta={t('desktop.history.meta')}
			title={t('desktop.nav.history')}
		/>
	);
}

function SingleImages() {
	const { t } = useTranslation();
	return (
		<SeriesList
			empty={{ title: t('desktop.singleImages.emptyTitle'), text: t('desktop.singleImages.emptyText') }}
			keep={(row) => row.origin.kind === 'oneOff'}
			meta={t('desktop.singleImages.meta')}
			title={t('desktop.nav.singleImages')}
		/>
	);
}

export const HistoryRoute = createLazyRoute('/history')({ component: History });
export const SingleImagesRoute = createLazyRoute('/single-images')({ component: SingleImages });
