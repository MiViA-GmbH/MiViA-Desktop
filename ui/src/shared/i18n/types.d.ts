import type en from './locales/desktop/en.json';

declare module 'i18next' {
	interface CustomTypeOptions {
		defaultNS: 'desktop';
		resources: { desktop: typeof en };
		returnNull: false;
	}
}
