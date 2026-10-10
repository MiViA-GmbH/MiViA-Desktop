import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import type { Language, LocaleView } from '../ipc/bindings';
import { setFormatLocale } from '../lib/format';
import en from './locales/desktop/en.json';

async function bundle(language: Language) {
	return language === 'de' ? (await import('./locales/desktop/de.json')).default : en;
}

export async function startI18n(locale: LocaleView): Promise<void> {
	setFormatLocale(locale.formatLocale);
	await i18n.use(initReactI18next).init({
		lng: locale.language,
		fallbackLng: 'en',
		supportedLngs: ['en', 'de'],
		ns: ['desktop'],
		defaultNS: 'desktop',
		resources: { en: { desktop: en }, [locale.language]: { desktop: await bundle(locale.language) } },
		partialBundledLanguages: true,
		returnNull: false,
		returnEmptyString: false,
		interpolation: { escapeValue: false },
		initAsync: false,
	});
}

export async function changeLanguage(locale: LocaleView): Promise<void> {
	setFormatLocale(locale.formatLocale);
	i18n.addResourceBundle(locale.language, 'desktop', await bundle(locale.language), true, true);
	await i18n.changeLanguage(locale.language);
}
