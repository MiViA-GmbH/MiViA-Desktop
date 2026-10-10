import { Select } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { LanguageSetting } from '../../shared/ipc/bindings';

const SETTINGS: LanguageSetting[] = ['auto', 'de', 'en'];

function isSetting(value: string | null): value is LanguageSetting {
	return SETTINGS.some((setting) => setting === value);
}

export function LanguageSelect({ setting }: { setting: LanguageSetting }) {
	const { t } = useTranslation();
	const save = useMutation({ mutationFn: (next: LanguageSetting) => commands.setLanguage(next) });
	const data = [
		{ value: 'auto', label: t('desktop.settings.languageAuto') },
		{ value: 'de', label: 'Deutsch' },
		{ value: 'en', label: 'English' },
	];
	return (
		<Select
			allowDeselect={false}
			data={data}
			label={t('desktop.settings.language')}
			maw={320}
			onChange={(value) => isSetting(value) && save.mutate(value)}
			value={setting}
		/>
	);
}
