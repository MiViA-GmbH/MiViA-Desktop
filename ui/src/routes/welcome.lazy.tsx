import { useQuery } from '@tanstack/react-query';
import { createLazyRoute, useParams } from '@tanstack/react-router';

import { setupQuery } from '../entities/setup/queries';
import { Wizard } from '../features/setup/wizard';

function Welcome() {
	const { step } = useParams({ from: '/welcome/$step' });
	const view = useQuery(setupQuery).data;
	return view ? <Wizard step={step} view={view} /> : null;
}

export const WelcomeRoute = createLazyRoute('/welcome/$step')({ component: Welcome });
