import { Anchor, Button, Group, Stack, Tabs, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute, useNavigate, useParams } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { foldersQuery } from '../entities/folder/queries';
import { useOrigin } from '../entities/series/origin';
import { historyQuery } from '../entities/series/queries';
import { NO_FILTER } from '../entities/series/search';
import { OpenSeriesCard } from '../features/folders/open-series-card';
import { SeriesTable } from '../features/series/series-table';
import { commands } from '../shared/ipc';
import type { FolderView } from '../shared/ipc/bindings';
import { EmptyState } from '../shared/ui/empty-state';
import { CollectingIcon, PausedIcon, ResumeIcon, SettingsIcon } from '../shared/ui/icons';
import { PageHeader } from '../shared/ui/page-header';
import { Panel } from '../shared/ui/panel';
import { PathText } from '../shared/ui/path-text';
import { NotFound } from './errors';

const EARLIER = 10;

function PauseButton({ folder }: { folder: FolderView }) {
	const { t } = useTranslation();
	const toggle = useMutation({
		mutationFn: () => (folder.paused ? commands.resumeFolder(folder.id) : commands.pauseFolder(folder.id)),
	});
	const ToggleIcon = folder.paused ? ResumeIcon : PausedIcon;
	return (
		<Button
			leftSection={<ToggleIcon size={16} stroke={ICON_STROKE} />}
			loading={toggle.isPending}
			onClick={() => toggle.mutate()}
			variant="default"
		>
			{folder.paused ? t('desktop.folder.resume') : t('desktop.folder.pause')}
		</Button>
	);
}

type FolderPageProps = { folder: FolderView; tab: 'series' | 'settings'; children: ReactNode };

function FolderPage({ folder, tab, children }: FolderPageProps) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const go = (next: string | null) =>
		navigate({
			to: next === 'settings' ? '/folders/$folderId/settings' : '/folders/$folderId',
			params: { folderId: folder.id },
		});
	return (
		<>
			<PageHeader
				actions={<PauseButton folder={folder} />}
				meta={
					<PathText inherit>
						{folder.reportFolder
							? t('desktop.folder.pathWithReports', { path: folder.path, reports: folder.reportFolder })
							: folder.path}
					</PathText>
				}
				title={folder.name}
			/>
			<Tabs keepMounted onChange={go} value={tab}>
				<Tabs.List mb="lg">
					<Tabs.Tab leftSection={<CollectingIcon size={16} stroke={ICON_STROKE} />} value="series">
						{t('desktop.folder.seriesTab')}
					</Tabs.Tab>
					<Tabs.Tab leftSection={<SettingsIcon size={16} stroke={ICON_STROKE} />} value="settings">
						{t('desktop.folder.settingsTab')}
					</Tabs.Tab>
				</Tabs.List>
				<Tabs.Panel value="series">{tab === 'series' && <Stack gap="lg">{children}</Stack>}</Tabs.Panel>
				<Tabs.Panel value="settings">{tab === 'settings' && <Stack gap="lg">{children}</Stack>}</Tabs.Panel>
			</Tabs>
		</>
	);
}

function useFolder(): { folder: FolderView | undefined; loaded: boolean } {
	const { folderId } = useParams({ strict: false });
	const data = useQuery(foldersQuery).data;
	return { folder: data?.folders.find((folder) => folder.id === folderId), loaded: data !== undefined };
}

function EarlierSeries({ folder }: { folder: FolderView }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const origin = useOrigin([folder]);
	const filter = { ...NO_FILTER, folder: folder.id };
	const rows = (useQuery(historyQuery(filter)).data?.rows ?? []).slice(0, EARLIER).map((row) => row.summary);
	if (!folder.openSeries && rows.length === 0) {
		return <EmptyState text={t('desktop.folder.noSeriesText')} title={t('desktop.folder.noSeriesTitle')} />;
	}
	if (rows.length === 0) {
		return null;
	}
	return (
		<Panel
			extra={
				<Anchor
					component="button"
					onClick={() => navigate({ to: '/history', search: { folder: folder.id } })}
					size="sm"
				>
					{t('desktop.overview.allInHistory')}
				</Anchor>
			}
			title={t('desktop.folder.earlierSeries')}
		>
			<SeriesTable origin={origin} rows={rows} />
		</Panel>
	);
}

function FolderSeries() {
	const { folder, loaded } = useFolder();
	if (!folder) {
		return loaded ? <NotFound /> : null;
	}
	return (
		<FolderPage folder={folder} tab="series">
			<OpenSeriesCard folder={folder} />
			<EarlierSeries folder={folder} />
		</FolderPage>
	);
}

function FolderSettings() {
	const { t } = useTranslation();
	const { folder, loaded } = useFolder();
	if (!folder) {
		return loaded ? <NotFound /> : null;
	}
	const minutes = Math.max(1, Math.round(folder.quietSeconds / 60));
	return (
		<FolderPage folder={folder} tab="settings">
			<Panel title={t('desktop.folder.reportsPanel')}>
				<Stack gap="xs">
					<Group gap="xs">
						<Text fw={500} size="sm">
							{t('desktop.folder.reportFolder')}
						</Text>
						<PathText c="stone.7" size="sm">
							{folder.reportFolder ?? t('desktop.folder.besideImages')}
						</PathText>
					</Group>
				</Stack>
			</Panel>
			<Panel title={t('desktop.folder.seriesPanel')}>
				<Text size="sm">{t('desktop.folder.quietSetting', { minutes })}</Text>
			</Panel>
		</FolderPage>
	);
}

export const FolderSeriesRoute = createLazyRoute('/app/folders/$folderId')({ component: FolderSeries });
export const FolderSettingsRoute = createLazyRoute('/app/folders/$folderId/settings')({ component: FolderSettings });
