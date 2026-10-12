import { Button, Group, Stack, Switch, Text } from '@mantine/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FieldListDialog } from '../../entities/privacy/field-list-dialog';
import { PRIVACY, privacyQuery } from '../../entities/privacy/queries';
import { SentDataDialog } from '../../entities/privacy/sent-data-dialog';
import { commands } from '../../shared/ipc';
import type { PrivacyView } from '../../shared/ipc/bindings';

function CopyId({ label, value }: { label: string; value: string | null }) {
	const { t } = useTranslation();
	const copy = (text: string) =>
		navigator.clipboard.writeText(text).then(() => {
			const message = t('desktop.privacy.copied');
			import('@mantine/notifications').then(({ notifications }) => notifications.show({ message, role: 'status' }));
		});
	return (
		<Group gap="sm" justify="space-between" wrap="nowrap">
			<Stack gap={0} miw={0}>
				<Text c="stone.7" size="xs">
					{label}
				</Text>
				<Text ff="monospace" size="sm" style={{ overflowWrap: 'anywhere' }}>
					{value ?? t('desktop.privacy.notRead')}
				</Text>
			</Stack>
			{value !== null && (
				<Button
					aria-label={`${t('desktop.privacy.copy')}: ${label}`}
					onClick={() => copy(value)}
					size="xs"
					variant="default"
				>
					{t('desktop.privacy.copy')}
				</Button>
			)}
		</Group>
	);
}

function TelemetrySwitch({ view }: { view: PrivacyView }) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const set = useMutation({
		mutationFn: (enabled: boolean) => commands.setTelemetry(enabled),
		onSuccess: (next) => client.setQueryData(PRIVACY, next),
	});
	const description = view.locked ? t('desktop.wizard.common.policy') : t('desktop.privacy.telemetryNever');
	return (
		<Stack gap="xs">
			<Switch
				checked={view.enabled}
				description={description}
				disabled={view.locked || set.isPending}
				label={t('desktop.privacy.telemetry')}
				onChange={(event) => set.mutate(event.currentTarget.checked)}
			/>
			{!view.enabled && (
				<Text c="stone.7" size="xs">
					{t('desktop.privacy.offNote')}
				</Text>
			)}
		</Stack>
	);
}

type Dialog = 'fields' | 'sent' | null;

export function PrivacyPanel() {
	const { t } = useTranslation();
	const view = useQuery(privacyQuery).data;
	const [dialog, setDialog] = useState<Dialog>(null);
	if (!view) {
		return null;
	}
	return (
		<Stack gap="md">
			<TelemetrySwitch view={view} />
			<Stack gap="xs">
				<Text fw={500} size="sm">
					{t('desktop.privacy.idsTitle')}
				</Text>
				<CopyId label={t('desktop.privacy.installId')} value={view.installId} />
				<CopyId label={t('desktop.privacy.machineId')} value={view.machineId} />
				<Text c="stone.7" size="xs">
					{t('desktop.privacy.idsHelp')}
				</Text>
			</Stack>
			<Group gap="sm">
				<Button onClick={() => setDialog('fields')} variant="default">
					{t('desktop.privacy.whatIsSent')}
				</Button>
				<Button onClick={() => setDialog('sent')} variant="default">
					{t('desktop.privacy.viewSent')}
				</Button>
			</Group>
			{dialog === 'fields' && <FieldListDialog onClose={() => setDialog(null)} />}
			{dialog === 'sent' && <SentDataDialog onClose={() => setDialog(null)} />}
		</Stack>
	);
}
