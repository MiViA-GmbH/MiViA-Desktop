import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';

import { notifyUpdated } from '../../entities/support/toasts';
import { updateQuery } from '../../entities/update/queries';
import { commands } from '../../shared/ipc';

export function UpdatedHost() {
	const applied = useQuery(updateQuery).data?.applied ?? null;
	useEffect(() => {
		if (applied === null) {
			return;
		}
		notifyUpdated(applied, () => commands.openTarget({ kind: 'link', link: 'releases' }));
		commands.dismissUpdateApplied();
	}, [applied]);
	return null;
}
