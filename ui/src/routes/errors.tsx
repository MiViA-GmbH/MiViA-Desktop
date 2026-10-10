import { Anchor } from '@mantine/core';
import { Link } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { EmptyState } from '../shared/ui/empty-state';

export function NotFound() {
	const { t } = useTranslation();
	return (
		<EmptyState
			action={
				<Anchor component={Link} size="sm" to="/">
					{t('desktop.errors.toOverview')}
				</Anchor>
			}
			text={t('desktop.errors.notFoundText')}
			title={t('desktop.errors.notFoundTitle')}
		/>
	);
}

export function RouteError() {
	const { t } = useTranslation();
	return <EmptyState text={t('desktop.errors.loadFailedText')} title={t('desktop.errors.loadFailedTitle')} />;
}
