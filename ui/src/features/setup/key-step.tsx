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
import { SignInTabs } from '../../shared/ui/sign-in-tabs';
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

function Offline({ details, host }: { details: string; host: string }) {
	const { t } = useTranslation();
	return (
		<Notice tone="warn">
			<Stack align="flex-start" gap="xs">
				<Text inherit>
					{t('desktop.wizard.key.offline', { host })} {t('desktop.wizard.key.autoRetry')}
				</Text>
				<Button onClick={() => navigator.clipboard.writeText(details)} size="xs" variant="default">
					{t('desktop.wizard.key.copyForIt')}
				</Button>
			</Stack>
		</Notice>
	);
}

function useSettle(onOutcome: (outcome: ConnectOutcome) => void) {
	const client = useQueryClient();
	return async (outcome: ConnectOutcome) => {
		onOutcome(outcome);
		await client.invalidateQueries({ queryKey: setupQuery.queryKey });
		await client.invalidateQueries({ queryKey: catalogueQuery.queryKey });
	};
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

function useKeyEntry() {
	const [outcome, setOutcome] = useState<ConnectOutcome | null>(null);
	const [key, setKey] = useState('');
	const [editing, setEditing] = useState(false);
	const input = useRef<HTMLInputElement>(null);
	const settle = useSettle((next) => {
		setOutcome(next);
		setEditing(next.kind !== 'connected');
		setKey(next.kind === 'connected' ? '' : key);
		if (next.kind !== 'connected') {
			input.current?.focus();
		}
	});
	const connect = useMutation({
		mutationFn: (typed: string | null) => commands.connectAccessKey(typed),
		onSuccess: settle,
	});
	useRetry(connect.data ?? null, () => connect.mutate(null));
	const clear = () => {
		connect.reset();
		setOutcome(null);
	};
	return { outcome, key, setKey, editing, setEditing, input, settle, connect, clear };
}

function NotConnected({ view, entry }: { view: SetupView; entry: ReturnType<typeof useKeyEntry> }) {
	const { t } = useTranslation();
	const imported = useQuery<ConnectOutcome | null>({ queryKey: IMPORT_KEY, queryFn: () => null, enabled: false }).data;
	const { outcome, key, setKey, input, settle, connect, clear } = entry;
	const error = connect.data?.kind === 'offline' ? null : outcomeError(outcome, view.host, t);
	return (
		<>
			{imported?.kind === 'rejected' && <Notice tone="warn">{t('desktop.wizard.key.importedInvalid')}</Notice>}
			<SignInTabs
				error={error}
				host={view.host}
				keyPanel={
					<>
						<Text c="stone.8" size="sm">
							{t('desktop.wizard.key.lead')}
						</Text>
						<KeyField
							error={error}
							inputRef={input}
							onChange={setKey}
							onPaste={(text) => text.trim().length >= 32 && connect.mutate(text)}
							value={key}
						/>
						{connect.data?.kind === 'offline' && <Offline details={connect.data.details} host={view.host} />}
						<KeyHelp host={view.host} />
					</>
				}
				onOutcome={settle}
				onTabChange={clear}
			/>
			<Text c="stone.7" size="xs">
				{t('desktop.wizard.common.help', { contact: view.supportContact })}
			</Text>
		</>
	);
}

export function KeyStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { label, loading, advance } = useAdvance(view, 'key');
	const entry = useKeyEntry();
	const { outcome, key, editing, setEditing, connect } = entry;
	const connection = editing ? null : view.connection;
	const next = () => (connection ? advance() : connect.mutate(key));
	return (
		<WizardFrame
			onBack={() => go(previousStep(view, 'key'))}
			primary={{
				label,
				onClick: next,
				loading: loading || connect.isPending,
				disabled: !connection && key.trim() === '',
			}}
			step="key"
			title={t('desktop.wizard.key.title')}
			view={view}
		>
			{connection ? (
				<>
					<Connected connection={connection} outcome={outcome} />
					<Anchor component="button" ta="left" w="fit-content" onClick={() => setEditing(true)} size="sm" type="button">
						{connection.notes.includes('browser')
							? t('desktop.wizard.key.differentAccount')
							: t('desktop.wizard.key.other')}
					</Anchor>
				</>
			) : (
				<NotConnected entry={entry} view={view} />
			)}
		</WizardFrame>
	);
}
