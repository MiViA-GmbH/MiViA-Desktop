import { Anchor, Radio, Stack, Text } from '@mantine/core';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../../entities/account/queries';
import { commands, isAppError } from '../../shared/ipc';
import type { LanguageSetting, SetupView } from '../../shared/ipc/bindings';
import { nextStep } from './steps';
import { useGo, useSetupView } from './use-setup';
import { WizardFrame } from './wizard-frame';

const SETTINGS: LanguageSetting[] = ['auto', 'de', 'en'];

function isSetting(value: string): value is LanguageSetting {
	return SETTINGS.some((setting) => setting === value);
}

function LanguageChoice({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const locale = useQuery(localeQuery).data;
	const save = useMutation({ mutationFn: (setting: LanguageSetting) => commands.setLanguage(setting) });
	if (view.locks.language || !locale) {
		return null;
	}
	return (
		<Radio.Group
			label={t('desktop.wizard.welcome.language')}
			onChange={(value) => isSetting(value) && save.mutate(value)}
			value={locale.setting}
		>
			<Stack gap="xs" mt="xs">
				<Radio label={t('desktop.wizard.welcome.langAuto', { language: view.detectedLanguage })} value="auto" />
				<Radio label="Deutsch" value="de" />
				<Radio label="English" value="en" />
			</Stack>
		</Radio.Group>
	);
}

function LegacyLink({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const store = useSetupView();
	const [error, setError] = useState(false);
	const pick = useMutation({
		mutationFn: () => commands.pickSetupFolder('legacy'),
		meta: { inline: true },
		onSuccess: store,
		onError: (failure) => setError(isAppError(failure)),
	});
	if (view.legacy || !view.locks.legacySearch) {
		return null;
	}
	return (
		<Stack gap={4}>
			<Anchor component="button" ta="left" w="fit-content" onClick={() => pick.mutate()} size="sm" type="button">
				{t('desktop.wizard.welcome.legacyManual')}
			</Anchor>
			{error && (
				<Text c="red.7" size="sm">
					{t('desktop.wizard.legacy.notInFolder')}
				</Text>
			)}
		</Stack>
	);
}

export function WelcomeStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	return (
		<WizardFrame
			primary={{ label: t('desktop.wizard.welcome.start'), onClick: () => go(nextStep(view, 'welcome')) }}
			step="welcome"
			title={t('desktop.wizard.welcome.title')}
			view={view}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.welcome.lead')}
			</Text>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.welcome.need')}
			</Text>
			<LanguageChoice view={view} />
			<Text c="stone.7" size="sm">
				{t('desktop.wizard.welcome.noKey')}
			</Text>
			<LegacyLink view={view} />
			{view.host !== 'app.mivia.ai' && (
				<Text c="stone.7" size="xs">
					{t('desktop.wizard.common.server', { host: view.host })}
				</Text>
			)}
		</WizardFrame>
	);
}
