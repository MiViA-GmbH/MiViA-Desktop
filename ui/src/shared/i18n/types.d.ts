import type en from './locales/desktop/en.json';
import type setup from './locales/setup/en.json';

declare module 'i18next' {
	interface CustomTypeOptions {
		defaultNS: 'desktop';
		resources: { desktop: typeof en & typeof setup };
		returnNull: false;
	}
}
