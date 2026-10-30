import { Anchor, Button, Stack, Tabs, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands } from '../ipc';
import type { ConnectOutcome } from '../ipc/bindings';
import { Notice } from './notice';

const LINK_MARK = '\u0001';

type SignInTabsProps = {
	error: string | null;
	host: string;
	keyPanel: ReactNode;
	onOutcome: (outcome: ConnectOutcome) => void;
	onTabChange: () => void;
};

function KeyLink({ host }: { host: string }) {
	const { t } = useTranslation();
	const open = useMutation({ mutationFn: () => commands.openTarget({ kind: 'link', link: 'accessKeys' }) });
	const [before, after] = t('desktop.wizard.key.keyLink', { link: LINK_MARK }).split(LINK_MARK);
	return (
		<Text c="stone.7" size="sm">
			{before}
			<Anchor component="button" inherit onClick={() => open.mutate()} type="button">
				{host}/api-keys
			</Anchor>
			{after}
		</Text>
	);
}

type BrowserPanelProps = {
	error: string | null;
	pending: boolean;
	onCancel: () => void;
	onStart: () => void;
	onDifferentAccount: () => void;
};

function BrowserPanel({ error, pending, onCancel, onStart, onDifferentAccount }: BrowserPanelProps) {
	const { t } = useTranslation();
	return (
		<Stack align="flex-start" gap="sm">
			<Button onClick={pending ? onCancel : onStart} type="button" variant={pending ? 'default' : 'light'}>
				{pending ? t('desktop.wizard.key.browserCancel') : t('desktop.wizard.key.browserOpen')}
			</Button>
			{!pending && (
				<Anchor component="button" onClick={onDifferentAccount} size="sm" type="button">
					{t('desktop.wizard.key.differentAccount')}
				</Anchor>
			)}
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
	const browser = useMutation({
		mutationFn: (differentAccount: boolean) => commands.startBrowserSignIn(differentAccount),
		onSuccess: onOutcome,
	});
	const cancel = () => commands.cancelSignIn();
	useEffect(() => () => void commands.cancelSignIn(), []);
	const change = (next: string | null) => {
		if (browser.isPending) {
			cancel();
		}
		setTab(next);
		onTabChange();
	};
	return (
		<Tabs keepMounted={false} onChange={change} value={tab}>
			<Tabs.List grow>
				<Tabs.Tab value="browser">{t('desktop.wizard.key.tabBrowser')}</Tabs.Tab>
				<Tabs.Tab disabled value="code">
					{t('desktop.wizard.key.tabCode')}
				</Tabs.Tab>
				<Tabs.Tab value="key">{t('desktop.wizard.key.label')}</Tabs.Tab>
			</Tabs.List>
			<Tabs.Panel pt="md" value="browser">
				<BrowserPanel
					error={error}
					onCancel={cancel}
					onDifferentAccount={() => browser.mutate(true)}
					onStart={() => browser.mutate(false)}
					pending={browser.isPending}
				/>
			</Tabs.Panel>
			<Tabs.Panel pt="md" value="key">
				<Stack gap="md">
					{keyPanel}
					<KeyLink host={host} />
				</Stack>
			</Tabs.Panel>
		</Tabs>
	);
}
