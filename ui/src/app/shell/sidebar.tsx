import { Avatar, Box, Divider, Image, NavLink, ScrollArea, Stack, Text } from '@mantine/core';
import { ICON_STROKE, MiviaLogo } from '@mivia-ai/theme';
import favicon from '@mivia-ai/theme/favicon.svg';
import { useQuery } from '@tanstack/react-query';
import { Link, useLocation } from '@tanstack/react-router';
import { lazy, type ReactNode, Suspense } from 'react';
import { useTranslation } from 'react-i18next';

import { accountQuery } from '../../entities/account/queries';
import { foldersQuery } from '../../entities/folder/queries';
import { folderStatus } from '../../entities/folder/status';
import { holdsQuery } from '../../entities/hold/queries';
import type { Tone } from '../../shared/ui/tone';
import { NAVIGATION } from './navigation';
import classes from './sidebar.module.css';

const SideLink = NavLink.withProps({ classNames: classes });
const RailTooltip = lazy(() => import('./rail-tooltip').then((module) => ({ default: module.RailTooltip })));
const ICON = { size: 18, stroke: ICON_STROKE };

type LinkProps = Record<string, unknown>;

type ItemProps = {
	link: (props: LinkProps) => ReactNode;
	active: boolean;
	label: string;
	icon: ReactNode;
	rail: boolean;
	description?: string;
	tone?: Tone;
};

function Item({ link: renderLink, active, label, icon, rail, description, tone }: ItemProps) {
	const link = (
		<SideLink
			active={active}
			aria-label={rail ? label : undefined}
			data-tone={tone}
			description={rail ? undefined : description}
			label={rail ? undefined : label}
			leftSection={icon}
			renderRoot={renderLink}
		/>
	);
	if (!rail) {
		return link;
	}
	return (
		<Suspense fallback={link}>
			<RailTooltip label={description ? `${label}: ${description}` : label}>{link}</RailTooltip>
		</Suspense>
	);
}

function Folders({ rail }: { rail: boolean }) {
	const { t } = useTranslation();
	const path = useLocation({ select: (location) => location.pathname });
	const folders = useQuery(foldersQuery).data?.folders ?? [];
	const offline = (useQuery(holdsQuery).data?.holds ?? []).length > 0;
	return (
		<ScrollArea flex={1} pl="xs" type="never">
			<Stack gap={2}>
				{folders.map((folder) => {
					const status = folderStatus(folder, offline, t);
					const FolderIcon = status.icon;
					return (
						<Item
							active={path.startsWith(`/folders/${folder.id}`)}
							description={status.label}
							icon={<FolderIcon {...ICON} />}
							key={folder.id}
							label={folder.name}
							link={(props) => <Link params={{ folderId: folder.id }} to="/folders/$folderId" {...props} />}
							rail={rail}
							tone={status.tone}
						/>
					);
				})}
			</Stack>
		</ScrollArea>
	);
}

function Account({ rail }: { rail: boolean }) {
	const { t } = useTranslation();
	const path = useLocation({ select: (location) => location.pathname });
	const account = useQuery(accountQuery).data;
	if (!account) {
		return null;
	}
	return (
		<Item
			active={path === '/settings/account'}
			description={t('desktop.nav.account')}
			icon={<Avatar color="jade" name={account.displayName} size={26} variant="white" />}
			label={account.displayName}
			link={(props) => <Link params={{ section: 'account' }} to="/settings/$section" {...props} />}
			rail={rail}
		/>
	);
}

export function Sidebar({ rail }: { rail: boolean }) {
	const { t } = useTranslation();
	const path = useLocation({ select: (location) => location.pathname });
	const links = (group: 'main' | 'bottom') =>
		NAVIGATION.filter((entry) => entry.group === group).map((entry) => {
			const EntryIcon = entry.icon;
			return (
				<Item
					active={entry.active(path)}
					icon={<EntryIcon {...ICON} />}
					key={entry.label}
					label={t(entry.label)}
					link={entry.link}
					rail={rail}
				/>
			);
		});
	return (
		<Stack component="nav" gap={0} h="100%">
			<Box pb="md" pt="lg" px={rail ? 'xs' : 'lg'}>
				{rail ? (
					<Image alt="MiViA" h={32} src={favicon} w={32} />
				) : (
					<MiviaLogo aria-label="MiViA" role="img" variant="white" w={176} />
				)}
			</Box>
			<Stack gap={2} pl="xs">
				{links('main')}
			</Stack>
			{!rail && (
				<Text c="stone.4" fw={500} mt="lg" pb={4} px="lg" size="xs">
					{t('desktop.nav.inputFolders')}
				</Text>
			)}
			<Folders rail={rail} />
			<Divider color="jade.7" />
			<Stack gap={2} pl="xs" py="xs">
				{links('bottom')}
				<Account rail={rail} />
			</Stack>
		</Stack>
	);
}
