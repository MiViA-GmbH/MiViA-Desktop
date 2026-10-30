import { Anchor, Button, Stack, Tabs, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands, events } from '../ipc';
import type { ConnectOutcome, KnownLink } from '../ipc/bindings';
import { Notice } from './notice';

const LINK_MARK = '\u0001';

type SignInTabsProps = {
	error: string | null;
	host: string;
	keyPanel: ReactNode;
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
	error: string | null;
	host: string;
	onRetry: () => void;
	pending: boolean;
};

function CodePanel({ code, error, host, onRetry, pending }: CodePanelProps) {
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
			{error && !pending && (
				<>
					<Notice tone="warn">{error}</Notice>
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
	const start = useMutation({
		mutationFn: async () => {
			setCode(null);
			const stop = await events.signInCode.listen(({ payload }) => setCode(payload.userCode));
			try {
				return await commands.startCodeSignIn();
			} finally {
				stop();
			}
		},
		onSuccess: onOutcome,
	});
	return { code, start };
}

type BrowserPanelProps = { error: string | null; pending: boolean; onCancel: () => void; onStart: () => void };

function BrowserPanel({ error, pending, onCancel, onStart }: BrowserPanelProps) {
	const { t } = useTranslation();
	return (
		<Stack align="flex-start" gap="sm">
			<Button onClick={pending ? onCancel : onStart} type="button" variant={pending ? 'default' : 'light'}>
				{pending ? t('desktop.wizard.key.browserCancel') : t('desktop.wizard.key.browserOpen')}
			</Button>
			<Text c="stone.8" role="status" size="sm">
				{pending && t('desktop.wizard.key.browserWaiting')}
			</Text>
			{error && !pending && <Notice tone="warn">{error}</Notice>}
		</Stack>
	);
}

export function SignInTabs({ error, host, keyPanel, onOutcome, onTabChange }: SignInTabsProps) {
	const { t } = useTranslation();
	const [tab, setTab] = useState<string | null>('browser');
	const browser = useMutation({ mutationFn: () => commands.startBrowserSignIn(), onSuccess: onOutcome });
	const { code, start } = useCodeSignIn(onOutcome);
	const cancel = () => commands.cancelSignIn();
	useEffect(() => () => void commands.cancelSignIn(), []);
	const change = (next: string | null) => {
		if (next === tab) {
			return;
		}
		setTab(next);
		onTabChange();
		if (next === 'code') {
			start.mutate();
		} else if (browser.isPending || start.isPending) {
			cancel();
		}
	};
	return (
		<Tabs keepMounted={false} onChange={change} value={tab}>
			<Tabs.List grow>
				<Tabs.Tab value="browser">{t('desktop.wizard.key.tabBrowser')}</Tabs.Tab>
				<Tabs.Tab value="code">{t('desktop.wizard.key.tabCode')}</Tabs.Tab>
				<Tabs.Tab value="key">{t('desktop.wizard.key.label')}</Tabs.Tab>
			</Tabs.List>
			<Tabs.Panel pt="md" value="browser">
				<BrowserPanel error={error} onCancel={cancel} onStart={() => browser.mutate()} pending={browser.isPending} />
			</Tabs.Panel>
			<Tabs.Panel pt="md" value="code">
				<CodePanel code={code} error={error} host={host} onRetry={() => start.mutate()} pending={start.isPending} />
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
