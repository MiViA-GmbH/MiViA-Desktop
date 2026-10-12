import { Alert, Button, Group, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import type { Icon } from '@tabler/icons-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';

import { holdsQuery, networkQuery } from '../../entities/hold/queries';
import { UPDATE } from '../../entities/update/queries';
import { commands } from '../../shared/ipc';
import type { BlockKind, HoldKind, HoldView, KnownLink, NetworkView } from '../../shared/ipc/bindings';
import { formatTime } from '../../shared/lib/format';
import { KeyIcon, LicenseIcon, OfflineIcon, TooOldIcon } from '../../shared/ui/icons';
import classes from './hold-banner.module.css';

type Look = { icon: Icon; color: 'orange' | 'red'; rank: number; title: string; text: string };

const LOOK = {
	accountBlocked: {
		icon: KeyIcon,
		color: 'red',
		rank: 0,
		title: 'desktop.hold.accountBlocked.title',
		text: 'desktop.hold.accountBlocked.text',
	},
	adminScope: {
		icon: KeyIcon,
		color: 'red',
		rank: 1,
		title: 'desktop.hold.adminScope.title',
		text: 'desktop.hold.adminScope.text',
	},
	accessKey: { icon: KeyIcon, color: 'red', rank: 2, title: 'desktop.hold.accessKey.title', text: 'desktop.hold.safe' },
	license: {
		icon: LicenseIcon,
		color: 'red',
		rank: 3,
		title: 'desktop.hold.license.title',
		text: 'desktop.hold.license.text',
	},
	clientTooOld: {
		icon: TooOldIcon,
		color: 'red',
		rank: 4,
		title: 'desktop.hold.updateRequired',
		text: 'desktop.hold.safe',
	},
	networkBlocked: {
		icon: OfflineIcon,
		color: 'orange',
		rank: 5,
		title: 'desktop.hold.networkBlocked.title',
		text: 'desktop.hold.continues',
	},
	offline: {
		icon: OfflineIcon,
		color: 'orange',
		rank: 6,
		title: 'desktop.hold.offline.title',
		text: 'desktop.hold.continues',
	},
	serviceUnavailable: {
		icon: OfflineIcon,
		color: 'orange',
		rank: 7,
		title: 'desktop.hold.serviceUnavailable.title',
		text: 'desktop.hold.continues',
	},
} as const satisfies Record<HoldKind, Look>;

const WEB = {
	license: { link: 'license', label: 'desktop.hold.open.license' },
	adminScope: { link: 'accessKeys', label: 'desktop.hold.open.accessKeys' },
} as const satisfies Partial<Record<HoldKind, { link: KnownLink; label: string }>>;

function strongest(holds: HoldView[]): HoldView | undefined {
	return [...holds].sort((a, b) => LOOK[a.kind].rank - LOOK[b.kind].rank)[0];
}

const BLOCK_TEXT = {
	proxyAuth: 'desktop.hold.networkBlocked.proxyAuth',
	tls: 'desktop.hold.networkBlocked.tls',
	proxyRefused: 'desktop.hold.networkBlocked.proxyRefused',
	redirect: 'desktop.hold.networkBlocked.redirect',
} as const satisfies Record<BlockKind, string>;

function useBlock(hold: HoldView): NetworkView | undefined {
	const blocked = hold.kind === 'networkBlocked';
	return useQuery({ ...networkQuery(hold.since), enabled: blocked }).data;
}

function holdText(hold: HoldView, network: NetworkView | undefined, t: TFunction): string {
	if (hold.kind !== 'networkBlocked' || !network?.block) {
		return t(LOOK[hold.kind].text);
	}
	const key =
		network.block === 'tls' && !network.issuer ? 'desktop.hold.networkBlocked.tlsNoIssuer' : BLOCK_TEXT[network.block];
	return t(key, { host: network.host, issuer: network.issuer });
}

function CopyDetails() {
	const { t } = useTranslation();
	const copy = useMutation({
		mutationFn: async () => {
			await navigator.clipboard.writeText(await commands.getNetworkDetails());
			const message = t('desktop.hold.copied');
			const { notifications } = await import('@mantine/notifications');
			notifications.show({ message, role: 'status', withCloseButton: false });
		},
	});
	return (
		<Button flex="none" loading={copy.isPending} onClick={() => copy.mutate()} size="xs" variant="default">
			{t('desktop.hold.copyDetails')}
		</Button>
	);
}

function HoldAction({ kind }: { kind: HoldKind }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const check = useMutation({ mutationFn: () => commands.checkConnectionNow() });
	const open = useMutation({ mutationFn: (link: KnownLink) => commands.openTarget({ kind: 'link', link }) });
	const client = useQueryClient();
	const install = useMutation({
		mutationFn: () => commands.installUpdate(),
		onSuccess: (next) => client.setQueryData(UPDATE, next),
	});
	if (LOOK[kind].color === 'orange') {
		return (
			<Button flex="none" loading={check.isPending} onClick={() => check.mutate()} size="xs" variant="default">
				{t('desktop.hold.checkNow')}
			</Button>
		);
	}
	const enterKey = (
		<Button
			flex="none"
			onClick={() => navigate({ to: '/settings/$section', params: { section: 'account' } })}
			size="xs"
		>
			{t('desktop.hold.enterKey')}
		</Button>
	);
	if (kind === 'accessKey') {
		return enterKey;
	}
	if (kind === 'license' || kind === 'adminScope') {
		const link = WEB[kind];
		return (
			<>
				<Button
					flex="none"
					onClick={() => open.mutate(link.link)}
					size="xs"
					variant={kind === 'license' ? 'filled' : 'default'}
				>
					{t(link.label)}
				</Button>
				{kind === 'adminScope' && enterKey}
			</>
		);
	}
	if (kind === 'clientTooOld') {
		return (
			<Button flex="none" loading={install.isPending} onClick={() => install.mutate()} size="xs">
				{t('desktop.update.restart')}
			</Button>
		);
	}
	return null;
}

function Banner({ hold }: { hold: HoldView }) {
	const { t } = useTranslation();
	const network = useBlock(hold);
	const copyable = hold.kind === 'networkBlocked' && network?.block && network.block !== 'redirect';
	const look = LOOK[hold.kind];
	const HoldIcon = look.icon;
	return (
		<Alert
			bg={`${look.color}.0`}
			classNames={classes}
			color={look.color}
			icon={<HoldIcon size={20} stroke={ICON_STROKE} />}
			px="xl"
			radius={0}
			role="status"
		>
			<Group gap="md" justify="space-between" wrap="nowrap">
				<Text c="dark.7" size="sm">
					<Text fw={600} inherit span>
						{t(look.title, { time: formatTime(hold.since) })}
					</Text>{' '}
					{holdText(hold, network, t)}
				</Text>
				<Group flex="none" gap="xs" wrap="nowrap">
					{copyable && <CopyDetails />}
					<HoldAction kind={hold.kind} />
				</Group>
			</Group>
		</Alert>
	);
}

export function HoldBanner() {
	const { data } = useQuery(holdsQuery);
	const hold = strongest(data?.holds ?? []);
	if (!hold) {
		return null;
	}
	return <Banner hold={hold} />;
}
