import { Alert, Button, Group, Select, Stack, Switch, Text } from '@mantine/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { ReportButton } from '../../entities/support/report-button';
import { UPDATE, updateQuery } from '../../entities/update/queries';
import { commands } from '../../shared/ipc';
import type { UpdateChannel, UpdateSettingsInput, UpdateView } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';

const STATUS = {
	idle: null,
	checking: 'desktop.update.checking',
	upToDate: 'desktop.update.upToDate',
	available: 'desktop.update.available',
	downloading: 'desktop.update.downloading',
	ready: 'desktop.update.ready',
	failed: 'desktop.update.failed',
} as const satisfies Record<UpdateView['state'], string | null>;

function isChannel(value: string | null): value is UpdateChannel {
	return value === 'stable' || value === 'beta';
}

function useSaveSettings() {
	const client = useQueryClient();
	return useMutation({
		mutationFn: (input: UpdateSettingsInput) => commands.setUpdateSettings(input),
		onSuccess: (next) => client.setQueryData(UPDATE, next),
	});
}

function Status({ view }: { view: UpdateView }) {
	const { t } = useTranslation();
	const key = STATUS[view.state];
	return (
		<Stack gap={0}>
			{key && (
				<Text role="status" size="sm">
					{t(key, { version: view.version })}
				</Text>
			)}
			{view.lastCheckAt && (
				<Text c="stone.7" size="xs">
					{t('desktop.update.lastCheck', { time: formatDateTime(view.lastCheckAt) })}
				</Text>
			)}
		</Stack>
	);
}

function Actions({ view }: { view: UpdateView }) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const check = useMutation({
		mutationFn: () => commands.checkForUpdates(),
		onSuccess: (next) => client.setQueryData(UPDATE, next),
	});
	const install = useMutation({
		mutationFn: () => commands.installUpdate(),
		onSuccess: (next) => client.setQueryData(UPDATE, next),
	});
	const installable = view.state === 'ready' || view.state === 'available';
	return (
		<Group gap="sm">
			<Button loading={check.isPending || view.state === 'checking'} onClick={() => check.mutate()} variant="default">
				{t('desktop.update.check')}
			</Button>
			{installable && (
				<Button loading={install.isPending} onClick={() => install.mutate()}>
					{t('desktop.update.restart')}
				</Button>
			)}
		</Group>
	);
}

function FailedInstall({ version }: { version: string }) {
	const { t } = useTranslation();
	return (
		<Alert color="red" variant="light">
			<Group gap="md" justify="space-between" wrap="nowrap">
				<Text size="sm">{t('desktop.update.installFailed', { version })}</Text>
				<ReportButton />
			</Group>
		</Alert>
	);
}

export function UpdatesPanel() {
	const { t } = useTranslation();
	const view = useQuery(updateQuery).data;
	const save = useSaveSettings();
	if (!view) {
		return null;
	}
	const locked = view.policy !== null;
	const automatic = locked ? view.policy === 'on' : view.automatic;
	const channels = [
		{ value: 'stable', label: t('desktop.update.stable') },
		{ value: 'beta', label: t('desktop.update.beta') },
	];
	return (
		<Stack gap="md">
			{view.failedInstall && <FailedInstall version={view.failedInstall} />}
			<Text size="sm">{t('desktop.update.current', { version: view.current })}</Text>
			<Select
				allowDeselect={false}
				data={channels}
				description={t('desktop.update.channelNote')}
				disabled={save.isPending}
				label={t('desktop.update.channel')}
				maw={320}
				onChange={(value) => isChannel(value) && save.mutate({ channel: value, automatic: view.automatic })}
				value={view.channel}
			/>
			<Switch
				checked={automatic}
				description={locked ? t('desktop.wizard.common.policy') : undefined}
				disabled={locked || save.isPending}
				label={t('desktop.update.automatic')}
				onChange={(event) => save.mutate({ channel: view.channel, automatic: event.currentTarget.checked })}
			/>
			<Status view={view} />
			<Actions view={view} />
		</Stack>
	);
}
