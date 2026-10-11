import { useTranslation } from 'react-i18next';

export function useLabel(): (key: string | undefined) => string {
	const { t } = useTranslation();
	return (key) => (key ? t(`model.${key}`, { defaultValue: key, nsSeparator: false, keySeparator: false }) : '');
}
