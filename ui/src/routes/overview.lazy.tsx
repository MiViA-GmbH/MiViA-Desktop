import { Anchor, Box, Button, Group } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute, Link } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { attentionQuery } from '../entities/attention/queries';
import { foldersQuery } from '../entities/folder/queries';
import { holdsQuery } from '../entities/hold/queries';
import { useOrigin } from '../entities/series/origin';
import { historyQuery } from '../entities/series/queries';
import { updateQuery } from '../entities/update/queries';
import { FolderList } from '../features/folders/folder-list';
import { NeedsAttentionPanel } from '../features/needs-attention/needs-attention-panel';
import { SeriesTable } from '../features/series/series-table';
import { commands } from '../shared/ipc';
import type { FolderView } from '../shared/ipc/bindings';
import { formatTime } from '../shared/lib/format';
import { EmptyState } from '../shared/ui/empty-state';
import { AddFolderIcon, DoneIcon, ResumeIcon } from '../shared/ui/icons';
import { PageHeader } from '../shared/ui/page-header';
import { Panel } from '../shared/ui/panel';

const RECENT = 5;

function lastSaved(folders: FolderView[]): string | undefined {
	return folders
		.flatMap(({ lastSeries }) =>
			lastSeries?.closedAt && lastSeries.counts.reportsSaved > 0 ? [lastSeries.closedAt] : [],
		)
		.sort()
		.at(-1);
}

function ResumeAll({ folders }: { folders: FolderView[] }) {
	const { t } = useTranslation();
	const resume = useMutation({
		mutationFn: () => Promise.all(folders.map((folder) => commands.resumeFolder(folder.id))),
	});
	return (
		<Button
			leftSection={<ResumeIcon size={16} stroke={ICON_STROKE} />}
			loading={resume.isPending}
			onClick={() => resume.mutate()}
		>
			{t('desktop.overview.resumeAll')}
		</Button>
	);
}

function Calm({ saved }: { saved: string | undefined }) {
	const { t } = useTranslation();
	return (
		<Group gap={6}>
			<Box c="forest.6" lh={0}>
				<DoneIcon size={16} stroke={ICON_STROKE} />
			</Box>
			{saved ? t('desktop.overview.calmSaved', { time: formatTime(saved) }) : t('desktop.overview.calm')}
		</Group>
	);
}

function useMeta(folders: FolderView[], attention: number, held: boolean): { meta: ReactNode; actions?: ReactNode } {
	const { t } = useTranslation();
	const paused = folders.length > 0 && folders.every((folder) => folder.paused);
	if (paused) {
		return { meta: t('desktop.overview.pausedMeta'), actions: <ResumeAll folders={folders} /> };
	}
	if (attention > 0) {
		return { meta: t('desktop.overview.attentionMeta', { count: attention }) };
	}
	if (held) {
		return { meta: null };
	}
	return { meta: <Calm saved={lastSaved(folders)} /> };
}

function Overview() {
	const { t } = useTranslation();
	const folders = useQuery(foldersQuery).data?.folders;
	const attention = useQuery(attentionQuery).data;
	const offline = (useQuery(holdsQuery).data?.holds ?? []).length > 0;
	const required = useQuery(updateQuery).data?.required;
	const recent = (useQuery(historyQuery()).data?.rows ?? []).slice(0, RECENT).map((row) => row.summary);
	const origin = useOrigin(folders ?? []);
	const header = useMeta(folders ?? [], attention?.total ?? 0, offline || required?.signed === false);
	if (!folders) {
		return null;
	}
	if (folders.length === 0) {
		return (
			<>
				<PageHeader title={t('desktop.overview.title')} />
				<EmptyState
					action={
						<Button component={Link} leftSection={<AddFolderIcon size={16} stroke={ICON_STROKE} />} to="/folders/new">
							{t('desktop.nav.addFolder')}
						</Button>
					}
					hint={t('desktop.overview.emptyHint')}
					text={t('desktop.overview.emptyText')}
					title={t('desktop.overview.emptyTitle')}
				/>
			</>
		);
	}
	return (
		<>
			<PageHeader actions={header.actions} meta={header.meta} title={t('desktop.overview.title')} />
			{attention && <NeedsAttentionPanel view={attention} />}
			<Panel title={t('desktop.nav.inputFolders')}>
				<FolderList folders={folders} offline={offline} />
			</Panel>
			{recent.length > 0 && (
				<Panel
					extra={
						<Anchor component={Link} size="sm" to="/history">
							{t('desktop.overview.allInHistory')}
						</Anchor>
					}
					title={t('desktop.overview.recentSeries')}
				>
					<SeriesTable origin={origin} rows={recent} />
				</Panel>
			)}
		</>
	);
}

export const OverviewRoute = createLazyRoute('/app/')({ component: Overview });
