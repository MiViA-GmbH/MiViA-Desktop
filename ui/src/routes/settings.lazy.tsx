import { Divider, Flex, NavLink, Stack } from '@mantine/core';
import { useMediaQuery } from '@mantine/hooks';
import { useQuery } from '@tanstack/react-query';
import { createLazyRoute, Link, useParams } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../entities/account/queries';
import { updateQuery } from '../entities/update/queries';
import { PrivacyPanel } from '../features/privacy/privacy-panel';
import { AboutPanel } from '../features/settings/about-panel';
import { AccountPanel } from '../features/settings/account-panel';
import { BackupPanel } from '../features/settings/backup-panel';
import { AutostartSwitch, ExplorerSwitch } from '../features/settings/integration-panel';
import { LanguageSelect } from '../features/settings/language-select';
import { SupportPanel } from '../features/support/support-panel';
import { UpdatesPanel } from '../features/updates/updates-panel';
import { Dot } from '../shared/ui/dot';
import { PageHeader } from '../shared/ui/page-header';
import { Panel } from '../shared/ui/panel';
import { NotFound } from './errors';
import classes from './settings.module.css';

const SECTIONS = {
	account: 'desktop.settings.account',
	general: 'desktop.settings.general',
	explorer: 'desktop.settings.explorer',
	updates: 'desktop.settings.updates',
	privacy: 'desktop.settings.privacy',
	support: 'desktop.settings.support',
	about: 'desktop.settings.about',
} as const;

type Section = keyof typeof SECTIONS;

function isSection(value: string): value is Section {
	return value in SECTIONS;
}

function General() {
	const locale = useQuery(localeQuery).data;
	return (
		<Stack gap="md">
			{locale && <LanguageSelect setting={locale.setting} />}
			<AutostartSwitch />
			<Divider />
			<BackupPanel />
		</Stack>
	);
}

const PANELS = {
	account: { title: 'desktop.settings.account', body: AccountPanel },
	general: { title: 'desktop.settings.general', body: General },
	explorer: { title: 'desktop.settings.explorer', body: ExplorerSwitch },
	updates: { title: 'desktop.settings.updates', body: UpdatesPanel },
	privacy: { title: 'desktop.settings.privacy', body: PrivacyPanel },
	support: { title: 'desktop.settings.support', body: SupportPanel },
	about: { title: 'desktop.settings.about', body: AboutPanel },
} as const;

function Section({ section }: { section: Section }) {
	const { t } = useTranslation();
	const Body = PANELS[section].body;
	return (
		<Panel title={t(PANELS[section].title)}>
			<Body />
		</Panel>
	);
}

function Settings() {
	const { t } = useTranslation();
	const { section } = useParams({ from: '/app/settings/$section' });
	const narrow = useMediaQuery('(max-width: 959px)') ?? false;
	const notice = useQuery(updateQuery).data?.notice ?? false;
	if (!isSection(section)) {
		return <NotFound />;
	}
	return (
		<>
			<PageHeader title={t('desktop.nav.settings')} />
			<Flex align={narrow ? 'stretch' : 'flex-start'} direction={narrow ? 'column' : 'row'} gap={narrow ? 'md' : 'lg'}>
				<Flex direction={narrow ? 'row' : 'column'} gap={2} w={narrow ? undefined : 200} wrap="wrap">
					{(Object.keys(SECTIONS) as Section[]).map((key) => (
						<NavLink
							active={key === section}
							classNames={classes}
							key={key}
							label={t(SECTIONS[key])}
							renderRoot={(props) => <Link params={{ section: key }} to="/settings/$section" {...props} />}
							rightSection={key === 'updates' && notice ? <Dot label={t('desktop.update.notice')} /> : undefined}
							w={narrow ? 'auto' : undefined}
						/>
					))}
				</Flex>
				<Stack flex={1} miw={0}>
					<Section section={section} />
				</Stack>
			</Flex>
		</>
	);
}

export const SettingsRoute = createLazyRoute('/app/settings/$section')({ component: Settings });
