import { Button, Checkbox, Group, Modal, Stack, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import i18n from 'i18next';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { notifyStatus } from '../../entities/support/toasts';
import { commands } from '../../shared/ipc';
import type { DeleteHistoryView } from '../../shared/ipc/bindings';

type DeleteSubject = {
	series: string;
	name: string;
	image?: string;
	images: number;
};

type DeleteDialogProps = { subject: DeleteSubject; onClose: () => void; onDeleted?: () => void };

function outcomeText(view: DeleteHistoryView): string {
	return [
		i18n.t('desktop.delete.done', { count: view.images }),
		view.filesFailed > 0 ? i18n.t('desktop.delete.filesFailed', { count: view.filesFailed }) : '',
		view.serverFailed > 0 ? i18n.t('desktop.delete.serverFailed', { count: view.serverFailed }) : '',
	]
		.filter((sentence) => sentence.length > 0)
		.join(' ');
}

function Choice({
	checked,
	label,
	help,
	onChange,
}: {
	checked: boolean;
	label: string;
	help: string;
	onChange: (next: boolean) => void;
}) {
	return (
		<Checkbox
			checked={checked}
			description={help}
			label={label}
			onChange={(event) => onChange(event.currentTarget.checked)}
		/>
	);
}

export function DeleteDialog({ subject, onClose, onDeleted }: DeleteDialogProps) {
	const { t } = useTranslation();
	const [reportFiles, setReportFiles] = useState(false);
	const [server, setServer] = useState(true);
	const image = subject.image ?? null;
	const remove = useMutation({
		mutationFn: () => commands.deleteHistory({ series: subject.series, image, reportFiles, server }),
		onSuccess: (view) => {
			const failed = view.filesFailed > 0 || view.serverFailed > 0;
			notifyStatus('desktop.toast.deleted', outcomeText(view), failed ? { color: 'orange' } : {});
			onClose();
			onDeleted?.();
		},
	});
	const text = image
		? t('desktop.delete.imageText', { name: subject.name })
		: t('desktop.delete.seriesText', { name: subject.name, count: subject.images });
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{t(image ? 'desktop.delete.imageTitle' : 'desktop.delete.seriesTitle')}
		</Text>
	);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.delete.cancel') }}
			onClose={onClose}
			opened
			title={title}
		>
			<Stack gap="md">
				<Text size="sm">{text}</Text>
				<Text c="stone.7" size="sm">
					{t('desktop.delete.logNote')}
				</Text>
				<Choice
					checked={reportFiles}
					help={t('desktop.delete.reportFilesHelp')}
					label={t('desktop.delete.reportFiles')}
					onChange={setReportFiles}
				/>
				<Choice
					checked={server}
					help={t('desktop.delete.serverHelp')}
					label={t('desktop.delete.server')}
					onChange={setServer}
				/>
				<Group gap="sm" justify="flex-end">
					<Button data-autofocus onClick={onClose} variant="default">
						{t('desktop.delete.cancel')}
					</Button>
					<Button color="red" loading={remove.isPending} onClick={() => remove.mutate()}>
						{t('desktop.delete.confirm')}
					</Button>
				</Group>
			</Stack>
		</Modal>
	);
}
