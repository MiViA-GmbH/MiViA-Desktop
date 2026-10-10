import { Button, Paper } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { foldersQuery } from '../entities/folder/queries';
import { useOrigin } from '../entities/series/origin';
import { historyQuery } from '../entities/series/queries';
import { SeriesTable } from '../features/series/series-table';
import { commands } from '../shared/ipc';
import type { BatchSummary } from '../shared/ipc/bindings';
import { EmptyState } from '../shared/ui/empty-state';
import { AddImagesIcon } from '../shared/ui/icons';
import { PageHeader } from '../shared/ui/page-header';

type ListProps = {
	title: string;
	meta: string;
	empty: { title: string; text: string };
	keep: (row: BatchSummary) => boolean;
	actions?: ReactNode;
};

function SeriesList({ title, meta, empty, keep, actions }: ListProps) {
	const page = useQuery(historyQuery).data;
	const origin = useOrigin(useQuery(foldersQuery).data?.folders ?? []);
	const rows = [...(page?.head ?? []), ...(page?.rows ?? [])].filter(keep);
	return (
		<>
			<PageHeader actions={actions} meta={meta} title={title} />
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
	return (
		<SeriesList
			actions={<AddImages />}
			empty={{ title: t('desktop.singleImages.emptyTitle'), text: t('desktop.singleImages.emptyText') }}
			keep={(row) => row.origin.kind === 'oneOff'}
			meta={t('desktop.singleImages.meta')}
			title={t('desktop.nav.singleImages')}
		/>
	);
}

export const HistoryRoute = createLazyRoute('/app/history')({ component: History });
export const SingleImagesRoute = createLazyRoute('/app/single-images')({ component: SingleImages });
