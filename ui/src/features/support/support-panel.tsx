import { Anchor, Button, Divider, Group, Modal, Stack, Text } from '@mantine/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { contactText, requestReport, SUPPORT, supportQuery } from '../../entities/support/queries';
import { commands } from '../../shared/ipc';
import type { SupportView } from '../../shared/ipc/bindings';
import { PendingReports, PolicyTable } from './support-lists';

function DeleteLogsDialog({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const remove = useMutation({
		mutationFn: () => commands.deleteLocalLogs(),
		onSuccess: (count) => {
			const message = t('desktop.support.deleted', { count });
			import('@mantine/notifications').then(({ notifications }) => notifications.show({ message, role: 'status' }));
			client.invalidateQueries({ queryKey: SUPPORT });
			onClose();
		},
	});
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{t('desktop.support.deleteTitle')}
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
				<Text size="sm">{t('desktop.support.deleteText')}</Text>
				<Group gap="sm" justify="flex-end">
					<Button data-autofocus onClick={onClose} variant="default">
						{t('desktop.custom.cancel')}
					</Button>
					<Button color="red" loading={remove.isPending} onClick={() => remove.mutate()} variant="subtle">
						{t('desktop.support.deleteConfirm')}
					</Button>
				</Group>
			</Stack>
		</Modal>
	);
}

function Contact({ view }: { view: SupportView }) {
	const { t } = useTranslation();
	const open = useMutation({ mutationFn: () => commands.openTarget({ kind: 'link', link: 'support' }) });
	return (
		<Stack gap={2}>
			<Text fw={500} size="sm">
				{t('desktop.support.contact')}
			</Text>
			<Anchor component="button" onClick={() => open.mutate()} size="sm" style={{ alignSelf: 'flex-start' }}>
				{contactText(view.contact)}
			</Anchor>
			<Text c="stone.7" size="xs">
				{t('desktop.support.version', { version: view.version, installId: view.installId })}
			</Text>
		</Stack>
	);
}

export function SupportPanel() {
	const { t } = useTranslation();
	const client = useQueryClient();
	const view = useQuery(supportQuery).data;
	const [deleting, setDeleting] = useState(false);
	if (!view) {
		return null;
	}
	const reporting = view.reporting !== 'off';
	return (
		<Stack gap="md">
			<Contact view={view} />
			{!reporting && <Text size="sm">{t('desktop.errors.reportingOff')}</Text>}
			<Group gap="sm">
				{reporting && (
					<Button onClick={() => requestReport(client, { trigger: 'manual', signature: null })}>
						{t('desktop.support.reportAction')}
					</Button>
				)}
				<Button onClick={() => setDeleting(true)} variant="default">
					{t('desktop.support.deleteLogs')}
				</Button>
			</Group>
			<PendingReports />
			<Divider />
			<PolicyTable policies={view.policies} />
			{deleting && <DeleteLogsDialog onClose={() => setDeleting(false)} />}
		</Stack>
	);
}
