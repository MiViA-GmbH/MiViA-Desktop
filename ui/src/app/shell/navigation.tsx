import type { Icon } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import type { ReactNode } from 'react';

import { HistoryIcon, OverviewIcon, SettingsIcon, SingleImagesIcon } from '../../shared/ui/icons';

export type NavigationEntry = {
	group: 'main' | 'bottom';
	icon: Icon;
	label: 'desktop.nav.overview' | 'desktop.nav.history' | 'desktop.nav.singleImages' | 'desktop.nav.settings';
	active: (path: string) => boolean;
	link: (props: Record<string, unknown>) => ReactNode;
};

export const NAVIGATION: NavigationEntry[] = [
	{
		group: 'main',
		icon: OverviewIcon,
		label: 'desktop.nav.overview',
		active: (path) => path === '/',
		link: (props) => <Link to="/" {...props} />,
	},
	{
		group: 'main',
		icon: HistoryIcon,
		label: 'desktop.nav.history',
		active: (path) => path.startsWith('/history'),
		link: (props) => <Link to="/history" {...props} />,
	},
	{
		group: 'main',
		icon: SingleImagesIcon,
		label: 'desktop.nav.singleImages',
		active: (path) => path.startsWith('/single-images'),
		link: (props) => <Link to="/single-images" {...props} />,
	},
	{
		group: 'bottom',
		icon: SettingsIcon,
		label: 'desktop.nav.settings',
		active: (path) => path.startsWith('/settings') && path !== '/settings/account',
		link: (props) => <Link params={{ section: 'general' }} to="/settings/$section" {...props} />,
	},
];
