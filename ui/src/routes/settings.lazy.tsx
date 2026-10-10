import { Group, NavLink, Stack, Text } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { createLazyRoute, Link, useParams } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { accountQuery, localeQuery } from '../entities/account/queries';
import { LanguageSelect } from '../features/settings/language-select';
import { PageHeader } from '../shared/ui/page-header';
import { Panel } from '../shared/ui/panel';
import { NotFound } from './errors';

const SECTIONS = { general: 'desktop.settings.general', account: 'desktop.settings.account' } as const;

type Section = keyof typeof SECTIONS;

function isSection(value: string): value is Section {
	return value in SECTIONS;
}

function General() {
	const { t } = useTranslation();
	const locale = useQuery(localeQuery).data;
	return <Panel title={t('desktop.settings.general')}>{locale && <LanguageSelect setting={locale.setting} />}</Panel>;
}

function Account() {
	const { t } = useTranslation();
	const account = useQuery(accountQuery).data;
	return (
		<Panel title={t('desktop.settings.account')}>
			<Stack gap="xs">
				<Text fw={600} size="sm">
					{account?.displayName ?? t('desktop.settings.noAccount')}
				</Text>
				{account && (
					<Text c="stone.7" size="sm">
						{account.serverUrl}
					</Text>
				)}
			</Stack>
		</Panel>
	);
}

function Settings() {
	const { t } = useTranslation();
	const { section } = useParams({ from: '/settings/$section' });
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
					{section === 'general' ? <General /> : <Account />}
				</Stack>
			</Group>
		</>
	);
}

export const SettingsRoute = createLazyRoute('/settings/$section')({ component: Settings });
