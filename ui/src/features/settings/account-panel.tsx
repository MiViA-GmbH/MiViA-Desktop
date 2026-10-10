import '../../shared/ui/forms.css';

import { Button, Group, Modal, Stack, Text } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { accountQuery } from '../../entities/account/queries';
import { setupQuery } from '../../entities/setup/queries';
import { commands } from '../../shared/ipc';
import type { AccountView, ConnectOutcome } from '../../shared/ipc/bindings';
import { outcomeError } from '../../shared/ui/key-error';
import { KeyField } from '../../shared/ui/key-field';
import { Notice } from '../../shared/ui/notice';

type SwitchProps = { outcome: ConnectOutcome | null; onDone: (outcome: ConnectOutcome) => void; onCancel: () => void };

function SwitchModal({ outcome, onDone, onCancel }: SwitchProps) {
	const { t } = useTranslation();
	const confirm = useMutation({ mutationFn: () => commands.switchAccount(), onSuccess: onDone });
	const opened = outcome?.kind === 'switchAccount';
	const names = opened ? { current: outcome.currentName, next: outcome.newName } : { current: '', next: '' };
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.account.switch.cancel') }}
			onClose={onCancel}
			opened={opened}
			title={t('desktop.account.switch.title')}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.account.switch.text', names)}
			</Text>
			<Group justify="flex-end" mt="lg">
				<Button data-autofocus onClick={onCancel} variant="default">
					{t('desktop.account.switch.cancel')}
				</Button>
				<Button color="red" loading={confirm.isPending} onClick={() => confirm.mutate()}>
					{t('desktop.account.switch.confirm')}
				</Button>
			</Group>
		</Modal>
	);
}

function SignOut({ account }: { account: AccountView }) {
	const { t } = useTranslation();
	const [opened, { open, close }] = useDisclosure(false);
	const leave = useMutation({ mutationFn: () => commands.signOut(), onSuccess: close });
	return (
		<>
			<Button color="red" onClick={open} variant="subtle">
				{t('desktop.account.signOut.button')}
			</Button>
			<Modal
				centered
				closeButtonProps={{ 'aria-label': t('desktop.account.signOut.cancel') }}
				onClose={close}
				opened={opened}
				title={t('desktop.account.signOut.title', { name: account.displayName })}
			>
				<Text c="stone.8" size="sm">
					{t('desktop.account.signOut.text', { name: account.displayName })}
				</Text>
				<Group justify="flex-end" mt="lg">
					<Button data-autofocus onClick={close} variant="default">
						{t('desktop.account.signOut.cancel')}
					</Button>
					<Button color="red" loading={leave.isPending} onClick={() => leave.mutate()}>
						{t('desktop.account.signOut.button')}
					</Button>
				</Group>
			</Modal>
		</>
	);
}

function KeyForm({ host }: { host: string }) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const [key, setKey] = useState('');
	const [outcome, setOutcome] = useState<ConnectOutcome | null>(null);
	const input = useRef<HTMLInputElement>(null);
	const settle = (next: ConnectOutcome) => {
		setOutcome(next);
		if (next.kind === 'connected') {
			setKey('');
			client.invalidateQueries({ queryKey: accountQuery.queryKey });
		} else {
			input.current?.focus();
		}
	};
	const save = useMutation({ mutationFn: () => commands.connectAccessKey(key), onSuccess: settle });
	return (
		<Stack gap="sm">
			<KeyField error={outcomeError(outcome, host, t)} inputRef={input} onChange={setKey} value={key} />
			<Group>
				<Button loading={save.isPending} onClick={() => save.mutate()}>
					{t('desktop.account.key.save')}
				</Button>
			</Group>
			{outcome?.kind === 'connected' && (
				<Notice tone="success">
					{t('desktop.account.key.saved', { name: outcome.connection.name })} {t('desktop.account.key.hint', { host })}
				</Notice>
			)}
			<SwitchModal onCancel={() => setOutcome(null)} onDone={settle} outcome={outcome} />
		</Stack>
	);
}

export function AccountPanel() {
	const { t } = useTranslation();
	const account = useQuery(accountQuery).data;
	const host = useQuery(setupQuery).data?.host ?? '';
	if (!account) {
		return (
			<Stack gap="sm">
				<Text fw={600} size="sm">
					{t('desktop.account.key.connect')}
				</Text>
				<Text c="stone.7" size="sm">
					{t('desktop.account.key.connectLead')}
				</Text>
				<KeyForm host={host} />
			</Stack>
		);
	}
	return (
		<Stack gap="md">
			<Group justify="space-between" wrap="nowrap">
				<Stack gap={2} miw={0}>
					<Text fw={600} size="sm">
						{account.displayName}
					</Text>
					<Text c="stone.7" size="sm">
						{account.email || account.serverUrl}
					</Text>
				</Stack>
				<SignOut account={account} />
			</Group>
			<Text fw={600} size="sm">
				{t('desktop.account.key.replace')}
			</Text>
			<KeyForm host={host} />
		</Stack>
	);
}
