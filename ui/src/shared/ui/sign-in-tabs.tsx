import { Anchor, Button, Stack, Tabs, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands, events } from '../ipc';
import type { ConnectOutcome, KnownLink } from '../ipc/bindings';
import { Notice } from './notice';
import { OfflineNotice } from './offline-notice';

const LINK_MARK = '\u0001';

type SignInTabsProps = {
	differentAccount?: boolean;
	error: string | null;
	host: string;
	keyPanel: ReactNode;
	offlineDetails: string | null;
	onOutcome: (outcome: ConnectOutcome) => void;
	onTabChange: () => void;
};

type LinkLineProps = { text: string; link: KnownLink; path: string };

function LinkLine({ text, link, path }: LinkLineProps) {
	const open = useMutation({ mutationFn: () => commands.openTarget({ kind: 'link', link }) });
	const [before, after] = text.split(LINK_MARK);
	return (
		<Text c="stone.7" size="sm">
			{before}
			<Anchor component="button" inherit onClick={() => open.mutate()} type="button">
				{path}
			</Anchor>
			{after}
		</Text>
	);
}

type CodePanelProps = {
	code: string | null;
	host: string;
	onRetry: () => void;
	pending: boolean;
	problem: ReactNode;
};

function CodePanel({ code, host, onRetry, pending, problem }: CodePanelProps) {
	const { t } = useTranslation();
	return (
		<Stack align="flex-start" gap="sm">
			{pending && code && (
				<>
					<LinkLine
						link="devicePage"
						path={`${host}/device`}
						text={t('desktop.wizard.key.codeLink', { link: LINK_MARK })}
					/>
					<Text ff="monospace" fw={600} size="xl">
						{code}
					</Text>
					<Button onClick={() => navigator.clipboard.writeText(code)} type="button" variant="default">
						{t('desktop.wizard.key.codeCopy')}
					</Button>
				</>
			)}
			<Text c="stone.8" role="status" size="sm">
				{pending && code && t('desktop.wizard.key.codeWaiting')}
			</Text>
			{problem && !pending && (
				<>
					{problem}
					<Button onClick={onRetry} type="button" variant="light">
						{t('desktop.wizard.common.tryAgain')}
					</Button>
				</>
			)}
		</Stack>
	);
}

function useCodeSignIn(onOutcome: (outcome: ConnectOutcome) => void) {
	const [code, setCode] = useState<string | null>(null);
	useEffect(() => {
		const unlisten = events.signInCode.listen(({ payload }) => setCode(payload.userCode));
		return () => {
			unlisten.then((stop) => stop());
		};
	}, []);
	const start = useMutation({
		mutationFn: () => commands.startCodeSignIn(),
		onMutate: () => setCode(null),
		onSuccess: onOutcome,
	});
	return { code, start };
}

type BrowserPanelProps = {
	offerDifferentAccount: boolean;
	pending: boolean;
	problem: ReactNode;
	onCancel: () => void;
	onStart: () => void;
	onDifferentAccount: () => void;
};

function BrowserPanel({
	offerDifferentAccount,
	pending,
	problem,
	onCancel,
	onStart,
	onDifferentAccount,
}: BrowserPanelProps) {
	const { t } = useTranslation();
	return (
		<Stack align="flex-start" gap="sm">
			<Button onClick={pending ? onCancel : onStart} type="button" variant={pending ? 'default' : 'light'}>
				{pending ? t('desktop.wizard.key.browserCancel') : t('desktop.wizard.key.browserOpen')}
			</Button>
			{!pending && offerDifferentAccount && (
				<Anchor component="button" onClick={onDifferentAccount} size="sm" type="button">
					{t('desktop.wizard.key.differentAccount')}
				</Anchor>
			)}
			<Text c="stone.8" role="status" size="sm">
				{pending && t('desktop.wizard.key.browserWaiting')}
			</Text>
			{!pending && problem}
		</Stack>
	);
}

function problemNotice(offlineDetails: string | null, error: string | null, host: string): ReactNode {
	if (offlineDetails) {
		return <OfflineNotice details={offlineDetails} host={host} />;
	}
	return error && <Notice tone="warn">{error}</Notice>;
}

function useSignInAttempts(onOutcome: (outcome: ConnectOutcome) => void, onTabChange: () => void) {
	const [tab, setTab] = useState<string | null>('browser');
	const settle = (outcome: ConnectOutcome) => {
		if (outcome.kind === 'cancelled') {
			return;
		}
		if (outcome.kind === 'signInUnavailable') {
			setTab('key');
		}
		if (outcome.kind === 'switchAccount') {
			setTab('browser');
		}
		onOutcome(outcome);
	};
	const browser = useMutation({
		mutationFn: (differentAccount: boolean) => commands.startBrowserSignIn(differentAccount),
		onSuccess: settle,
	});
	const { code, start } = useCodeSignIn(settle);
	const cancel = () => {
		commands.cancelSignIn();
		onTabChange();
	};
	useEffect(() => () => void commands.cancelSignIn(), []);
	const change = (next: string | null) => {
		if (next === tab) {
			return;
		}
		setTab(next);
		onTabChange();
		if (next === 'code') {
			start.mutate();
		} else {
			commands.cancelSignIn();
		}
	};
	return { tab, browser, code, start, cancel, change };
}

export function SignInTabs({
	differentAccount = false,
	error,
	host,
	keyPanel,
	offlineDetails,
	onOutcome,
	onTabChange,
}: SignInTabsProps) {
	const { t } = useTranslation();
	const { tab, browser, code, start, cancel, change } = useSignInAttempts(onOutcome, onTabChange);
	const problem = problemNotice(offlineDetails, error, host);
	return (
		<Tabs keepMounted={false} onChange={change} value={tab}>
			<Tabs.List grow>
				<Tabs.Tab value="browser">{t('desktop.wizard.key.tabBrowser')}</Tabs.Tab>
				<Tabs.Tab value="code">{t('desktop.wizard.key.tabCode')}</Tabs.Tab>
				<Tabs.Tab value="key">{t('desktop.wizard.key.label')}</Tabs.Tab>
			</Tabs.List>
			<Tabs.Panel pt="md" value="browser">
				<BrowserPanel
					offerDifferentAccount={!differentAccount}
					onCancel={cancel}
					onDifferentAccount={() => browser.mutate(true)}
					onStart={() => browser.mutate(differentAccount)}
					pending={browser.isPending}
					problem={problem}
				/>
			</Tabs.Panel>
			<Tabs.Panel pt="md" value="code">
				<CodePanel code={code} host={host} onRetry={() => start.mutate()} pending={start.isPending} problem={problem} />
			</Tabs.Panel>
			<Tabs.Panel pt="md" value="key">
				<Stack gap="md">
					{keyPanel}
					<LinkLine
						link="accessKeys"
						path={`${host}/api-keys`}
						text={t('desktop.wizard.key.keyLink', { link: LINK_MARK })}
					/>
				</Stack>
			</Tabs.Panel>
		</Tabs>
	);
}
