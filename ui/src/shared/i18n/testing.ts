import i18next, { type TFunction } from 'i18next';
import { setFormatLocale } from '../lib/format';
import en from './locales/desktop/en.json';

const instance = i18next.createInstance();
instance.init({
	lng: 'en',
	ns: ['desktop'],
	defaultNS: 'desktop',
	resources: { en: { desktop: en } },
	initAsync: false,
	interpolation: { escapeValue: false },
});

setFormatLocale('en-DE');

export const t: TFunction = instance.t;
