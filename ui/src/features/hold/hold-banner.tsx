import { Alert, Button, Group, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import type { Icon } from '@tabler/icons-react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { holdsQuery } from '../../entities/hold/queries';
import { commands } from '../../shared/ipc';
import type { HoldKind, HoldView } from '../../shared/ipc/bindings';
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
		title: 'desktop.hold.clientTooOld.title',
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

function strongest(holds: HoldView[]): HoldView | undefined {
	return [...holds].sort((a, b) => LOOK[a.kind].rank - LOOK[b.kind].rank)[0];
}

function HoldAction({ kind }: { kind: HoldKind }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const check = useMutation({ mutationFn: () => commands.checkConnectionNow() });
	if (LOOK[kind].color === 'orange') {
		return (
			<Button flex="none" loading={check.isPending} onClick={() => check.mutate()} size="xs" variant="default">
				{t('desktop.hold.checkNow')}
			</Button>
		);
	}
	if (kind === 'accessKey') {
		return (
			<Button
				flex="none"
				onClick={() => navigate({ to: '/settings/$section', params: { section: 'account' } })}
				size="xs"
			>
				{t('desktop.hold.enterKey')}
			</Button>
		);
	}
	return null;
}

export function HoldBanner() {
	const { t } = useTranslation();
	const { data } = useQuery(holdsQuery);
	const hold = strongest(data?.holds ?? []);
	if (!hold) {
		return null;
	}
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
					{t(look.text)}
				</Text>
				<HoldAction kind={hold.kind} />
			</Group>
		</Alert>
	);
}
