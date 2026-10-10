import { Anchor, Button, Collapse, Stack, Text } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useEffectEvent, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { catalogueQuery, setupQuery } from '../../entities/setup/queries';
import { commands } from '../../shared/ipc';
import type { ConnectionView, ConnectOutcome, SetupView } from '../../shared/ipc/bindings';
import { outcomeError } from '../../shared/ui/key-error';
import { KeyField } from '../../shared/ui/key-field';
import { Notice } from '../../shared/ui/notice';
import { previousStep } from './steps';
import { IMPORT_KEY, useAdvance } from './use-advance';
import { useGo } from './use-setup';
import { WizardFrame } from './wizard-frame';

const RETRY_MS = 30_000;

function Connected({ connection, outcome }: { connection: ConnectionView; outcome: ConnectOutcome | null }) {
	const { t } = useTranslation();
	const cleared = outcome?.kind === 'connected' && outcome.clipboardCleared;
	const imported = connection.notes.includes('imported');
	const stored = cleared ? t('desktop.wizard.key.stored') : t('desktop.wizard.key.storedOnly');
	return (
		<>
			<Notice
				title={t('desktop.wizard.key.connected', { name: connection.name, email: connection.email })}
				tone="success"
			>
				{imported ? t('desktop.wizard.key.imported') : stored}
			</Notice>
			{outcome?.kind === 'connected' && outcome.otherAccount && (
				<Notice tone="info">{t('desktop.wizard.key.otherAccount')}</Notice>
			)}
			{connection.notes.includes('storeFailed') && <Notice tone="warn">{t('desktop.wizard.key.storeFailed')}</Notice>}
			{connection.notes.includes('licenseInactive') && (
				<Notice tone="warn">{t('desktop.wizard.key.licenseInactive')}</Notice>
			)}
		</>
	);
}

function Offline({ details }: { details: string }) {
	const { t } = useTranslation();
	return (
		<Notice tone="warn">
			<Stack align="flex-start" gap="xs">
				<Text inherit>{t('desktop.wizard.key.autoRetry')}</Text>
				<Button onClick={() => navigator.clipboard.writeText(details)} size="xs" variant="default">
					{t('desktop.wizard.key.copyForIt')}
				</Button>
			</Stack>
		</Notice>
	);
}

function useConnect(onOutcome: (outcome: ConnectOutcome) => void) {
	const client = useQueryClient();
	return useMutation({
		mutationFn: (key: string | null) => commands.connectAccessKey(key),
		onSuccess: async (outcome) => {
			onOutcome(outcome);
			await client.invalidateQueries({ queryKey: setupQuery.queryKey });
			await client.invalidateQueries({ queryKey: catalogueQuery.queryKey });
		},
	});
}

function useRetry(outcome: ConnectOutcome | null, retry: () => void) {
	const onTimer = useEffectEvent(retry);
	const offline = outcome?.kind === 'offline';
	useEffect(() => {
		if (!offline) {
			return;
		}
		const timer = window.setInterval(onTimer, RETRY_MS);
		return () => window.clearInterval(timer);
	}, [offline]);
}

function KeyHelp({ host }: { host: string }) {
	const { t } = useTranslation();
	const [help, { toggle }] = useDisclosure(false);
	return (
		<>
			<Anchor
				aria-expanded={help}
				component="button"
				onClick={toggle}
				size="sm"
				ta="left"
				type="button"
				w="fit-content"
			>
				{t('desktop.wizard.key.helpTitle')}
			</Anchor>
			<Collapse expanded={help}>
				<Text c="stone.7" size="sm">
					{t('desktop.wizard.key.help', { host })}
				</Text>
			</Collapse>
		</>
	);
}

export function KeyStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { label, loading, advance } = useAdvance(view, 'key');
	const imported = useQuery<ConnectOutcome | null>({ queryKey: IMPORT_KEY, queryFn: () => null, enabled: false }).data;
	const [outcome, setOutcome] = useState<ConnectOutcome | null>(null);
	const [key, setKey] = useState('');
	const [editing, setEditing] = useState(false);
	const input = useRef<HTMLInputElement>(null);
	const connect = useConnect((next) => {
		setOutcome(next);
		setEditing(next.kind !== 'connected');
		setKey(next.kind === 'connected' ? '' : key);
		if (next.kind !== 'connected') {
			input.current?.focus();
		}
	});
	useRetry(outcome, () => connect.mutate(null));
	const connection = editing ? null : view.connection;
	const next = () => (connection ? advance() : connect.mutate(key));
	return (
		<WizardFrame
			onBack={() => go(previousStep(view, 'key'))}
			primary={{ label, onClick: next, loading: loading || connect.isPending }}
			step="key"
			title={t('desktop.wizard.key.title')}
			view={view}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.key.lead')}
			</Text>
			{connection ? (
				<>
					<Connected connection={connection} outcome={outcome} />
					<Anchor component="button" ta="left" w="fit-content" onClick={() => setEditing(true)} size="sm" type="button">
						{t('desktop.wizard.key.other')}
					</Anchor>
				</>
			) : (
				<>
					{imported?.kind === 'rejected' && <Notice tone="warn">{t('desktop.wizard.key.importedInvalid')}</Notice>}
					<KeyField
						error={outcomeError(outcome, view.host, t)}
						inputRef={input}
						onChange={setKey}
						onPaste={(text) => text.trim().length >= 32 && connect.mutate(text)}
						value={key}
					/>
					{outcome?.kind === 'offline' && <Offline details={outcome.details} />}
					<KeyHelp host={view.host} />
					<Text c="stone.7" size="xs">
						{t('desktop.wizard.common.help', { contact: view.supportContact })}
					</Text>
				</>
			)}
		</WizardFrame>
	);
}
