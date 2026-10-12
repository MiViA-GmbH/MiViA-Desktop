import { Button, Divider, Group, Stack, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import type { TFunction } from 'i18next';
import { Fragment, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { RelocateButton } from '../../entities/folder/relocate-button';
import { commands } from '../../shared/ipc';
import type { AttentionSeries, NeedsAttention, ProblemImage } from '../../shared/ipc/bindings';
import {
	FolderNotFoundIcon,
	ImageProblemIcon,
	ReportFailureIcon,
	WaitingIcon,
	WarningIcon,
} from '../../shared/ui/icons';
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

function CheckAgain({ folderId }: { folderId: string }) {
	const { t } = useTranslation();
	const check = useMutation({ mutationFn: () => commands.checkFolderAgain(folderId) });
	return (
		<Button loading={check.isPending} onClick={() => check.mutate()} size="xs" variant="subtle">
			{t('desktop.attention.checkAgain')}
		</Button>
	);
}

function WatchedElsewhereActions({ folderId }: { folderId: string }) {
	const { t } = useTranslation();
	const dismiss = useMutation({ mutationFn: () => commands.dismissWatchedElsewhere(folderId) });
	const pause = useMutation({ mutationFn: () => commands.pauseFolder(folderId) });
	return (
		<>
			<Button loading={dismiss.isPending} onClick={() => dismiss.mutate()} size="xs" variant="subtle">
				{t('desktop.attention.dismiss')}
			</Button>
			<Button loading={pause.isPending} onClick={() => pause.mutate()} size="xs" variant="default">
				{t('desktop.attention.pauseHere')}
			</Button>
		</>
	);
}

function minutesSince(iso: string): number {
	return Math.max(1, Math.round((Date.now() - Date.parse(iso)) / 60_000));
}

type Missing = Extract<NeedsAttention, { kind: 'folderNotFound' }>;
type Marker = Extract<NeedsAttention, { kind: 'watchedElsewhere' }>;

function missingText(item: Missing, t: TFunction): string {
	if (!item.unreachableSince) {
		return t('desktop.attention.folderNotFound');
	}
	return t('desktop.attention.networkMissing', { path: item.path, count: minutesSince(item.unreachableSince) });
}

function missingRow(item: Missing, t: TFunction): Row {
	return {
		key: `folder-${item.folderId}`,
		status: item.unreachableSince
			? { tone: 'waiting', icon: WaitingIcon, label: t('desktop.folder.state.waitingNetwork') }
			: { tone: 'problem', icon: FolderNotFoundIcon, label: t('desktop.folder.state.notFound') },
		subject: item.folderName,
		text: missingText(item, t),
		actions: (
			<>
				{item.unreachableSince && <CheckAgain folderId={item.folderId} />}
				{item.unreachableSince ? (
					<FolderAction folderId={item.folderId} />
				) : (
					<RelocateButton folderId={item.folderId} size="xs" />
				)}
			</>
		),
	};
}

function problemReason(state: string, t: TFunction): string | null {
	switch (state) {
		case 'rejected':
		case 'unreadable':
			return t('desktop.status.sourceFile.rejected');
		case 'unavailable':
			return t('desktop.status.computation.unavailable');
		default:
			return null;
	}
}

function problemsText(problems: number, first: ProblemImage | null, t: TFunction): string {
	if (!first) {
		return t('desktop.attention.seriesProblems', { count: problems });
	}
	const reason = problemReason(first.state, t);
	if (!reason) {
		return t('desktop.attention.problemName', { count: problems, name: first.name });
	}
	return t('desktop.attention.problemImage', { count: problems, name: first.name, reason });
}

function markerRow(item: Marker, t: TFunction): Row {
	return {
		key: `marker-${item.folderId}`,
		status: { tone: 'waiting', icon: WarningIcon, label: t('desktop.folder.state.watchedElsewhere') },
		subject: item.folderName,
		text: t('desktop.attention.watchedElsewhere', { machine: item.machine, user: item.user }),
		actions: <WatchedElsewhereActions folderId={item.folderId} />,
	};
}

function useRow(): (item: NeedsAttention) => Row {
	const { t } = useTranslation();
	return (item) => {
		switch (item.kind) {
			case 'folderNotFound':
				return missingRow(item, t);
			case 'watchedElsewhere':
				return markerRow(item, t);
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
					text: problemsText(item.problems, item.first, t),
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
