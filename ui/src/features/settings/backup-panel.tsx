import '../../shared/ui/forms.css';

import { Button, Group, Modal, Radio, Stack, Text } from '@mantine/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { BACKUPS, backupsQuery } from '../../entities/backup/queries';
import { commands } from '../../shared/ipc';
import type { BackupsView, BackupView } from '../../shared/ipc/bindings';
import { formatDateTime, formatDay } from '../../shared/lib/format';
import { PathText } from '../../shared/ui/path-text';

function useFolder() {
	const client = useQueryClient();
	return useMutation({
		mutationFn: async (clear: boolean) => {
			if (clear) {
				return commands.setBackupFolder(null);
			}
			const pick = await commands.pickFolder('reports');
			return pick ? commands.setBackupFolder(pick.pickId) : null;
		},
		onSuccess: (next: BackupsView | null) => next && client.setQueryData(BACKUPS, next),
	});
}

function BackupChoice({ backup, newest }: { backup: BackupView; newest: boolean }) {
	const { t } = useTranslation();
	const where = backup.inFolder ? t('desktop.backup.inFolder') : t('desktop.backup.onThisPc');
	const day = formatDay(backup.date);
	return (
		<Radio
			description={`${where} · ${t('desktop.backup.series', { count: backup.series })}`}
			label={newest ? t('desktop.backup.newest', { date: day }) : day}
			value={backup.id}
		/>
	);
}

function RestoreDialog({ backups, onClose }: { backups: BackupView[]; onClose: () => void }) {
	const { t } = useTranslation();
	const [chosen, setChosen] = useState<string | null>(null);
	const latest = backups.reduce((max, backup) => (backup.date > max ? backup.date : max), '');
	const restore = useMutation({
		mutationFn: (id: string) => commands.restoreBackup(id),
		onSuccess: () => window.location.reload(),
	});
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{t('desktop.backup.restoreTitle')}
		</Text>
	);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.custom.cancel') }}
			onClose={onClose}
			opened
			title={title}
		>
			<Stack gap="md">
				<Text size="sm">{t('desktop.backup.restoreText')}</Text>
				{backups.length === 0 && (
					<Text c="stone.7" size="sm">
						{t('desktop.backup.none')}
					</Text>
				)}
				<Radio.Group onChange={setChosen} value={chosen}>
					<Stack gap="sm">
						{backups.map((backup) => (
							<BackupChoice backup={backup} key={backup.id} newest={backup.date === latest} />
						))}
					</Stack>
				</Radio.Group>
				<Group gap="sm" justify="flex-end">
					<Button data-autofocus onClick={onClose} variant="default">
						{t('desktop.custom.cancel')}
					</Button>
					<Button disabled={!chosen} loading={restore.isPending} onClick={() => chosen && restore.mutate(chosen)}>
						{t('desktop.backup.restoreAction')}
					</Button>
				</Group>
			</Stack>
		</Modal>
	);
}

export function BackupPanel() {
	const { t } = useTranslation();
	const view = useQuery(backupsQuery).data;
	const folder = useFolder();
	const [restoring, setRestoring] = useState(false);
	if (!view) {
		return null;
	}
	return (
		<Stack gap="xs">
			<Text fw={500} size="sm">
				{t('desktop.backup.title')}
			</Text>
			{view.folder ? (
				<PathText size="sm">{view.folder}</PathText>
			) : (
				<Text size="sm">{t('desktop.backup.noFolder')}</Text>
			)}
			{view.lastCopy && (
				<Text c="stone.7" size="xs">
					{t('desktop.backup.lastCopy', { date: formatDateTime(view.lastCopy) })}
				</Text>
			)}
			<Group gap="sm">
				<Button loading={folder.isPending} onClick={() => folder.mutate(false)} size="xs" variant="default">
					{t('desktop.backup.choose')}
				</Button>
				{view.folder && (
					<Button onClick={() => folder.mutate(true)} size="xs" variant="subtle">
						{t('desktop.backup.remove')}
					</Button>
				)}
				<Button onClick={() => setRestoring(true)} size="xs" variant="subtle">
					{t('desktop.backup.restore')}
				</Button>
			</Group>
			{restoring && <RestoreDialog backups={view.backups} onClose={() => setRestoring(false)} />}
		</Stack>
	);
}
