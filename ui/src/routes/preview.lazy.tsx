import { Button } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { createLazyRoute, Link, useParams } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../entities/account/queries';
import { seriesItemsQuery, seriesQuery } from '../entities/series/queries';
import { PreviewBody } from '../features/result-preview/preview-body';
import { PageHeader } from '../shared/ui/page-header';
import { NotFound } from './errors';

function Preview() {
	const { t } = useTranslation();
	const { seriesId, itemId } = useParams({ from: '/app/preview/$seriesId/$itemId' });
	const language = useQuery(localeQuery).data?.language ?? 'en';
	const analyses = useQuery(seriesQuery(seriesId)).data?.analyses;
	const items = useQuery(seriesItemsQuery(seriesId)).data;
	if (!items || !analyses) {
		return null;
	}
	const item = items.items.find((candidate) => candidate.id === itemId);
	if (!item) {
		return <NotFound />;
	}
	return (
		<>
			<PageHeader
				actions={
					<Button
						renderRoot={(props) => <Link params={{ seriesId }} to="/series/$seriesId" {...props} />}
						variant="default"
					>
						{t('desktop.preview.back')}
					</Button>
				}
				title={item.label}
			/>
			<PreviewBody analyses={analyses} item={item} language={language} />
		</>
	);
}

export const PreviewRoute = createLazyRoute('/app/preview/$seriesId/$itemId')({ component: Preview });
