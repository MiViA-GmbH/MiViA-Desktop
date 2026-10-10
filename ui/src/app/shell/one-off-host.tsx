import { useQuery } from '@tanstack/react-query';
import { lazy, Suspense } from 'react';

import { oneOffQuery } from '../../entities/one-off/queries';
import { loadSetupTexts } from '../../shared/i18n/i18n';

const OneOffDialog = lazy(() =>
	import('../../features/one-off/one-off-dialog').then((module) => ({ default: module.OneOffDialog })),
);

export function OneOffHost() {
	const view = useQuery(oneOffQuery).data;
	const texts = useQuery({
		queryKey: ['setupTexts'],
		queryFn: () => loadSetupTexts().then(() => true),
		enabled: !!view,
	});
	if (!(view && texts.data)) {
		return null;
	}
	return (
		<Suspense>
			<OneOffDialog view={view} />
		</Suspense>
	);
}
