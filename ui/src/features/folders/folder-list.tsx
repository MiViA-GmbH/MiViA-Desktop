import { Divider, Group, Progress, Stack, Text, ThemeIcon, UnstyledButton } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useNavigate } from '@tanstack/react-router';
import { Fragment } from 'react';
import { useTranslation } from 'react-i18next';

import { folderStatus } from '../../entities/folder/status';
import type { FolderView } from '../../shared/ipc/bindings';
import { TONE_COLOR } from '../../shared/ui/tone';
import { FinishSeriesButton, LatestReportButton, ResumeButton } from './folder-actions';

function SeriesLine({ folder, words }: { folder: FolderView; words: string }) {
	const { t } = useTranslation();
	const series = folder.openSeries;
	if (!series || series.counts.files === 0) {
		return (
			<Text c="stone.7" size="xs" truncate>
				{words}
			</Text>
		);
	}
	const { files, done } = series.counts;
	return (
		<Group gap="sm" wrap="nowrap">
			<Text c="stone.7" size="xs" truncate>
				{t('desktop.folder.progress', { label: series.label, done, files })}
			</Text>
			<Progress aria-hidden flex={1} maw={140} size="sm" value={(done / files) * 100} />
		</Group>
	);
}

function FolderRow({ folder, offline }: { folder: FolderView; offline: boolean }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const status = folderStatus(folder, offline, t);
	const StateIcon = status.icon;
	const color = status.tone === 'done' ? 'stone' : TONE_COLOR[status.tone];
	return (
		<Group gap="md" py="sm" wrap="nowrap">
			<ThemeIcon color={color} size={36} variant="light">
				<StateIcon size={20} stroke={ICON_STROKE} />
			</ThemeIcon>
			<UnstyledButton
				flex={1}
				miw={0}
				onClick={() => navigate({ to: '/folders/$folderId', params: { folderId: folder.id } })}
			>
				<Stack gap={2}>
					<Text fw={600} size="sm" truncate>
						{folder.name}
					</Text>
					<SeriesLine folder={folder} words={status.label} />
				</Stack>
			</UnstyledButton>
			<Group gap="xs" justify="flex-end" wrap="nowrap">
				{folder.openSeries && <FinishSeriesButton seriesId={folder.openSeries.id} variant="subtle" />}
				{folder.paused && <ResumeButton folderId={folder.id} />}
				<LatestReportButton folder={folder} />
			</Group>
		</Group>
	);
}

export function FolderList({ folders, offline }: { folders: FolderView[]; offline: boolean }) {
	return (
		<Stack gap={0}>
			{folders.map((folder, index) => (
				<Fragment key={folder.id}>
					{index > 0 && <Divider />}
					<FolderRow folder={folder} offline={offline} />
				</Fragment>
			))}
		</Stack>
	);
}
