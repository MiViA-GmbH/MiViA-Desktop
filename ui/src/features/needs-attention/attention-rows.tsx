import { Button, Divider, Group, Stack, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { Fragment, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { AttentionSeries, NeedsAttention } from '../../shared/ipc/bindings';
import { FolderNotFoundIcon, ImageProblemIcon, ReportFailureIcon } from '../../shared/ui/icons';
import { StatusBadge } from '../../shared/ui/status-badge';
import type { Status } from '../../shared/ui/tone';

type Row = { key: string; status: Status; subject: string; text: string; actions: ReactNode };

function SeriesActions({ series, retry }: { series: AttentionSeries; retry?: string }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const dismiss = useMutation({ mutationFn: () => commands.clearSeriesAttention(series.id, 'dismissed') });
	const tryAgain = useMutation({ mutationFn: (part: string) => commands.tryAgainReportPart(part) });
	return (
		<>
			{retry ? (
				<Button onClick={() => tryAgain.mutate(retry)} size="xs" variant="subtle">
					{t('desktop.attention.tryAgain')}
				</Button>
			) : (
				<Button onClick={() => dismiss.mutate()} size="xs" variant="subtle">
					{t('desktop.attention.dismiss')}
				</Button>
			)}
			<Button
				onClick={() => navigate({ to: '/series/$seriesId', params: { seriesId: series.id } })}
				size="xs"
				variant="default"
			>
				{t('desktop.attention.openSeries')}
			</Button>
		</>
	);
}

function FolderAction({ folderId }: { folderId: string }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	return (
		<Button onClick={() => navigate({ to: '/folders/$folderId', params: { folderId } })} size="xs" variant="default">
			{t('desktop.attention.openFolder')}
		</Button>
	);
}

function useRow(): (item: NeedsAttention) => Row {
	const { t } = useTranslation();
	return (item) => {
		switch (item.kind) {
			case 'folderNotFound':
				return {
					key: `folder-${item.folderId}`,
					status: { tone: 'problem', icon: FolderNotFoundIcon, label: t('desktop.folder.state.notFound') },
					subject: item.folderName,
					text: t('desktop.attention.folderNotFound'),
					actions: <FolderAction folderId={item.folderId} />,
				};
			case 'reportNotSaved':
				return {
					key: `series-${item.series.id}`,
					status: { tone: 'problem', icon: ReportFailureIcon, label: t('desktop.status.report.notSaved') },
					subject: item.series.label,
					text: t('desktop.attention.reportNotSaved'),
					actions: <SeriesActions retry={item.partId} series={item.series} />,
				};
			case 'reportFailed':
				return {
					key: `series-${item.series.id}`,
					status: { tone: 'problem', icon: ReportFailureIcon, label: t('desktop.status.report.failed') },
					subject: item.series.label,
					text: t('desktop.attention.reportFailed'),
					actions: <SeriesActions retry={item.partId} series={item.series} />,
				};
			case 'reportStale':
				return {
					key: `series-${item.series.id}`,
					status: { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.report.stale') },
					subject: item.series.label,
					text: t('desktop.attention.reportStale'),
					actions: <SeriesActions series={item.series} />,
				};
			case 'seriesProblems':
				return {
					key: `series-${item.series.id}`,
					status: { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.batch.imageProblems') },
					subject: item.series.label,
					text: t('desktop.attention.seriesProblems', { count: item.problems }),
					actions: <SeriesActions series={item.series} />,
				};
			default:
				return item satisfies never;
		}
	};
}

export function AttentionRows({ items }: { items: NeedsAttention[] }) {
	const row = useRow();
	return (
		<Stack gap={0}>
			{items.map(row).map((entry, index) => (
				<Fragment key={entry.key}>
					{index > 0 && <Divider />}
					<Group gap="md" py="sm" wrap="nowrap">
						<Stack flex={1} gap={4} miw={0}>
							<Group gap="xs">
								<StatusBadge status={entry.status} />
								<Text fw={600} size="sm">
									{entry.subject}
								</Text>
							</Group>
							<Text c="stone.7" size="sm">
								{entry.text}
							</Text>
						</Stack>
						<Group gap="xs" justify="flex-end" wrap="nowrap">
							{entry.actions}
						</Group>
					</Group>
				</Fragment>
			))}
		</Stack>
	);
}
