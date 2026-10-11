import '../features/history/history.css';

import { ActionIcon, Button, Group, Menu, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconDots } from '@tabler/icons-react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute, useParams } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../entities/account/queries';
import { foldersQuery } from '../entities/folder/queries';
import { useOrigin } from '../entities/series/origin';
import { seriesEventsQuery, seriesItemsQuery, seriesQuery, seriesReportsQuery } from '../entities/series/queries';
import { CustomReportDialog } from '../features/history/custom-report-dialog';
import { useExport } from '../features/history/use-export';
import { ChangesPanel } from '../features/series/changes-panel';
import { ImagesPanel } from '../features/series/images-panel';
import { RenameSeries } from '../features/series/rename-series';
import { ReportsPanel } from '../features/series/reports-panel';
import { SeriesSummary } from '../features/series/series-summary';
import { commands } from '../shared/ipc';
import type { BatchSummary, SeriesReportsView } from '../shared/ipc/bindings';
import { formatDateTime, formatTime, sameDay } from '../shared/lib/format';
import { ShowInFolderIcon } from '../shared/ui/icons';
import { PageHeader } from '../shared/ui/page-header';
import { NotFound } from './errors';

function firstSaved(reports: SeriesReportsView | undefined): string | undefined {
	return reports?.reports.flatMap((report) => report.parts).find((part) => part.state === 'saved')?.id;
}

function ShowReports({ partId }: { partId: string | undefined }) {
	const { t } = useTranslation();
	const reveal = useMutation({ mutationFn: (id: string) => commands.openTarget({ kind: 'revealReport', partId: id }) });
	if (!partId) {
		return null;
	}
	return (
		<Button
			leftSection={<ShowInFolderIcon size={16} stroke={ICON_STROKE} />}
			onClick={() => reveal.mutate(partId)}
			variant="default"
		>
			{t('desktop.series.showReports')}
		</Button>
	);
}

function useMeta(series: BatchSummary): string {
	const { t } = useTranslation();
	const origin = useOrigin(useQuery(foldersQuery).data?.folders ?? [])(series);
	const from = formatDateTime(series.openedAt);
	const to = series.closedAt;
	if (!to) {
		return t('desktop.series.meta', { origin, span: t('desktop.series.since', { from }), count: series.counts.files });
	}
	const end = sameDay(series.openedAt, to) ? formatTime(to) : formatDateTime(to);
	const span = t('desktop.series.span', { from, to: end });
	return t('desktop.series.meta', { origin, span, count: series.counts.files });
}

function useOpened(series: BatchSummary | undefined): void {
	const id = series?.needsAttention ? series.id : undefined;
	useEffect(() => {
		if (id) {
			commands.clearSeriesAttention(id, 'opened');
		}
	}, [id]);
}

function SeriesMenu({ id }: { id: string }) {
	const { t } = useTranslation();
	const exporter = useExport();
	const [custom, setCustom] = useState(false);
	return (
		<>
			<Menu position="bottom-end" withinPortal>
				<Menu.Target>
					<ActionIcon aria-label={t('desktop.history.menu')} color="stone" size="lg" variant="default">
						<IconDots size={18} stroke={ICON_STROKE} />
					</ActionIcon>
				</Menu.Target>
				<Menu.Dropdown>
					<Menu.Item onClick={() => exporter.mutate({ filter: null, series: id })}>
						{t('desktop.history.exportSeries')}
					</Menu.Item>
					<Menu.Item onClick={() => setCustom(true)}>{t('desktop.history.createReport')}</Menu.Item>
				</Menu.Dropdown>
			</Menu>
			{custom && <CustomReportDialog onClose={() => setCustom(false)} series={[id]} />}
		</>
	);
}

function SeriesContent({ series, id }: { series: BatchSummary; id: string }) {
	const language = useQuery(localeQuery).data?.language ?? 'en';
	const view = useQuery(seriesQuery(id)).data;
	const reports = useQuery(seriesReportsQuery(id)).data;
	const items = useQuery(seriesItemsQuery(id)).data;
	const events = useQuery(seriesEventsQuery(id)).data;
	const analyses = view?.analyses ?? [];
	const actions = (
		<Group gap="sm" wrap="nowrap">
			<ShowReports partId={firstSaved(reports)} />
			<SeriesMenu id={id} />
		</Group>
	);
	return (
		<>
			<PageHeader
				actions={actions}
				meta={<Text inherit>{useMeta(series)}</Text>}
				title={series.label}
				titleExtra={<RenameSeries renamable={view?.renamable ?? false} series={series} />}
			/>
			<SeriesSummary series={series} />
			<ReportsPanel analyses={analyses} language={language} reports={reports?.reports ?? []} />
			<ImagesPanel analyses={analyses} items={items?.items ?? []} language={language} seriesId={id} />
			<ChangesPanel events={events?.events ?? []} />
		</>
	);
}

function Series() {
	const { seriesId } = useParams({ from: '/app/series/$seriesId' });
	const view = useQuery(seriesQuery(seriesId));
	useOpened(view.data?.summary);
	if (view.data === null) {
		return <NotFound />;
	}
	return view.data ? <SeriesContent id={seriesId} series={view.data.summary} /> : null;
}

export const SeriesRoute = createLazyRoute('/app/series/$seriesId')({ component: Series });
