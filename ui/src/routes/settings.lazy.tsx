import { Divider, Group, NavLink, Stack } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { createLazyRoute, Link, useParams } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../entities/account/queries';
import { PrivacyPanel } from '../features/privacy/privacy-panel';
import { AccountPanel } from '../features/settings/account-panel';
import { BackupPanel } from '../features/settings/backup-panel';
import { AutostartSwitch, ExplorerSwitch } from '../features/settings/integration-panel';
import { LanguageSelect } from '../features/settings/language-select';
import { SupportPanel } from '../features/support/support-panel';
import { PageHeader } from '../shared/ui/page-header';
import { Panel } from '../shared/ui/panel';
import { NotFound } from './errors';

const SECTIONS = {
	account: 'desktop.settings.account',
	general: 'desktop.settings.general',
	explorer: 'desktop.settings.explorer',
	privacy: 'desktop.settings.privacy',
	support: 'desktop.settings.support',
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
	privacy: { title: 'desktop.settings.privacy', body: PrivacyPanel },
	support: { title: 'desktop.settings.support', body: SupportPanel },
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
	if (!isSection(section)) {
		return <NotFound />;
	}
	return (
		<>
			<PageHeader title={t('desktop.nav.settings')} />
			<Group align="flex-start" gap="lg" wrap="nowrap">
				<Stack gap={2} w={200}>
					{(Object.keys(SECTIONS) as Section[]).map((key) => (
						<NavLink
							active={key === section}
							key={key}
							label={t(SECTIONS[key])}
							renderRoot={(props) => <Link params={{ section: key }} to="/settings/$section" {...props} />}
						/>
					))}
				</Stack>
				<Stack flex={1} miw={0}>
					<Section section={section} />
				</Stack>
			</Group>
		</>
	);
}

export const SettingsRoute = createLazyRoute('/app/settings/$section')({ component: Settings });
