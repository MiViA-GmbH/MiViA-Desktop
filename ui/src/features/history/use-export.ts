import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { HistoryFilter } from '../../shared/ipc/bindings';

type Target = { filter: HistoryFilter | null; series: string | null };

export function useExport() {
	const { t } = useTranslation();
	return useMutation({
		mutationFn: async ({ filter, series }: Target) => {
			const pick = await commands.pickFolder('reports');
			return pick ? commands.exportHistory(filter, series, pick.pickId) : null;
		},
		onSuccess: (view) => {
			if (!view) {
				return;
			}
			const message = t('desktop.history.exported', { count: view.files.length, folder: view.folder });
			import('@mantine/notifications').then(({ notifications }) =>
				notifications.show({ color: 'forest', message, role: 'status' }),
			);
		},
	});
}
