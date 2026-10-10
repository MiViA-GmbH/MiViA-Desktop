import '../../shared/ui/forms.css';

import { ActionIcon, Button, Group, Modal, Stack, Text, TextInput, Tooltip } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconPencil } from '@tabler/icons-react';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { BatchSummary } from '../../shared/ipc/bindings';

function RenameForm({ series, onClose }: { series: BatchSummary; onClose: () => void }) {
	const { t } = useTranslation();
	const [label, setLabel] = useState(series.label);
	const rename = useMutation({ mutationFn: () => commands.renameSeries(series.id, label), onSuccess: onClose });
	const empty = label.trim().length === 0;
	return (
		<Stack gap="md">
			<TextInput
				data-autofocus
				error={empty && t('desktop.series.nameRequired')}
				label={t('desktop.series.nameLabel')}
				maxLength={200}
				onChange={(event) => setLabel(event.currentTarget.value)}
				value={label}
			/>
			<Group gap="sm" justify="flex-end">
				<Button onClick={onClose} variant="default">
					{t('desktop.custom.cancel')}
				</Button>
				<Button disabled={empty} loading={rename.isPending} onClick={() => rename.mutate()}>
					{t('desktop.series.renameSave')}
				</Button>
			</Group>
		</Stack>
	);
}

export function RenameSeries({ series, renamable }: { series: BatchSummary; renamable: boolean }) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{t('desktop.series.rename')}
		</Text>
	);
	return (
		<>
			<Tooltip disabled={renamable} label={t('desktop.series.renameLocked')} multiline w={280}>
				<ActionIcon
					aria-label={t('desktop.series.rename')}
					color="stone"
					{...(renamable ? {} : { 'data-disabled': true })}
					onClick={() => renamable && setOpen(true)}
					variant="subtle"
				>
					<IconPencil size={18} stroke={ICON_STROKE} />
				</ActionIcon>
			</Tooltip>
			{open && (
				<Modal
					centered
					closeButtonProps={{ 'aria-label': t('desktop.custom.cancel') }}
					onClose={() => setOpen(false)}
					opened
					title={title}
				>
					<RenameForm onClose={() => setOpen(false)} series={series} />
				</Modal>
			)}
		</>
	);
}
