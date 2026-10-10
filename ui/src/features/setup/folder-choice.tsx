import { Button, Group, Paper, Radio, Stack, Text, ThemeIcon } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import type { Ref } from 'react';
import { useTranslation } from 'react-i18next';
import type { FolderNote, FolderPickView } from '../../shared/ipc/bindings';
import { FolderIcon } from '../../shared/ui/icons';
import { Notice } from '../../shared/ui/notice';
import { PathText } from '../../shared/ui/path-text';
import drop from './drop-zone.module.css';
import { blockText } from './folder-text';

type FolderChoiceProps = {
	folder: FolderPickView | null;
	error: string | null;
	busy: boolean;
	onChoose: () => void;
	chooseRef?: Ref<HTMLButtonElement>;
};

export function FolderChoice({ folder, error, busy, onChoose, chooseRef }: FolderChoiceProps) {
	const { t } = useTranslation();
	if (!folder) {
		return (
			<Stack gap="xs">
				<Paper classNames={drop} data-error={error ? true : undefined} p="lg">
					<Group justify="center">
						<Button aria-describedby="folder-error" loading={busy} onClick={onChoose} ref={chooseRef}>
							{t('desktop.wizard.folder.choose')}
						</Button>
					</Group>
				</Paper>
				{error && (
					<Text c="red.7" id="folder-error" size="sm">
						{error}
					</Text>
				)}
			</Stack>
		);
	}
	return (
		<Paper data-error={error ? true : undefined} p="sm" withBorder>
			<Group gap="sm" wrap="nowrap">
				<ThemeIcon color="jade" size={36} variant="light">
					<FolderIcon size={20} stroke={ICON_STROKE} />
				</ThemeIcon>
				<PathText flex={1} fw={500} size="sm">
					{folder.display}
				</PathText>
				<Button loading={busy} onClick={onChoose} ref={chooseRef} size="xs" variant="default">
					{t('desktop.wizard.folder.change')}
				</Button>
			</Group>
		</Paper>
	);
}

function NoteText({ note }: { note: FolderNote }) {
	const { t } = useTranslation();
	switch (note.kind) {
		case 'general':
			return <Notice tone="warn">{t('desktop.wizard.folder.general')}</Notice>;
		case 'oneDrive':
			return <Notice tone="warn">{t('desktop.wizard.folder.oneDrive')}</Notice>;
		case 'network':
			return <Notice tone="info">{t('desktop.wizard.folder.network')}</Notice>;
		case 'mappedDrive':
			return (
				<Notice tone="info">{t('desktop.wizard.folder.mappedDrive', { letter: note.letter, unc: note.unc })}</Notice>
			);
		default:
			return note satisfies never;
	}
}

function FolderNotes({ folder }: { folder: FolderPickView }) {
	return (
		<>
			{folder.notes.map((note) => (
				<NoteText key={note.kind} note={note} />
			))}
		</>
	);
}

type ExistingChoiceProps = {
	folder: FolderPickView;
	include: boolean;
	remaining: number | null;
	onChange: (include: boolean) => void;
};

function ExistingChoice({ folder, include, remaining, onChange }: ExistingChoiceProps) {
	const { t } = useTranslation();
	const count = folder.existing;
	const legend = folder.existingMore
		? t('desktop.wizard.folder.existingMany', { count })
		: t('desktop.wizard.folder.existing', { count });
	return (
		<Radio.Group label={legend} onChange={(value) => onChange(value === 'all')} value={include ? 'all' : 'new'}>
			<Stack gap="xs" mt="xs">
				<Radio label={t('desktop.wizard.folder.onlyNew')} value="new" />
				<Radio label={t('desktop.wizard.folder.alsoExisting', { count })} value="all" />
				{remaining !== null && (
					<Text c="stone.7" size="xs">
						{t('desktop.wizard.folder.uploadCost', { count, remaining })}
					</Text>
				)}
				<Text c="stone.7" size="xs">
					{t('desktop.wizard.folder.existingLater')}
				</Text>
			</Stack>
		</Radio.Group>
	);
}

type FolderDetailsProps = {
	folder: FolderPickView | null;
	include: boolean;
	remaining: number | null;
	onInclude: (include: boolean) => void;
};

export function FolderDetails({ folder, include, remaining, onInclude }: FolderDetailsProps) {
	const { t } = useTranslation();
	if (!folder) {
		return null;
	}
	if (folder.block) {
		return (
			<Text c="red.7" size="sm">
				{blockText(folder.block, t)}
			</Text>
		);
	}
	return (
		<>
			<FolderNotes folder={folder} />
			{folder.existing > 0 && (
				<ExistingChoice folder={folder} include={include} onChange={onInclude} remaining={remaining} />
			)}
		</>
	);
}
