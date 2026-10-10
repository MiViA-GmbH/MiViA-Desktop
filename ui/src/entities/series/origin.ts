import { useTranslation } from 'react-i18next';

import type { BatchSummary, FolderView } from '../../shared/ipc/bindings';

export function useOrigin(folders: FolderView[]): (series: BatchSummary) => string {
	const { t } = useTranslation();
	return (series) => {
		const { origin } = series;
		if (origin.kind === 'oneOff') {
			return t('desktop.nav.singleImages');
		}
		return folders.find((folder) => folder.id === origin.folderId)?.name ?? '';
	};
}
