import type { TFunction } from 'i18next';

import type { FolderView } from '../../shared/ipc/bindings';
import { formatTime } from '../../shared/lib/format';
import { CollectingIcon, DoneIcon, FolderNotFoundIcon, PausedIcon, WaitingIcon } from '../../shared/ui/icons';
import type { Status } from '../../shared/ui/tone';

export function folderStatus(folder: FolderView, offline: boolean, t: TFunction): Status {
	if (folder.missing) {
		return { tone: 'problem', icon: FolderNotFoundIcon, label: t('desktop.folder.state.notFound') };
	}
	if (folder.waitingSince) {
		return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.folder.state.waitingNetwork') };
	}
	if (folder.paused) {
		return { tone: 'off', icon: PausedIcon, label: t('desktop.folder.state.paused') };
	}
	if (offline && folder.openSeries) {
		return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.folder.state.waitingConnection') };
	}
	if (folder.openSeries) {
		return { tone: 'working', icon: CollectingIcon, label: t('desktop.folder.state.collecting') };
	}
	const saved = folder.lastSeries?.closedAt;
	if (saved && folder.lastSeries && folder.lastSeries.counts.reportsSaved > 0) {
		return { tone: 'done', icon: DoneIcon, label: t('desktop.folder.state.reportSaved', { time: formatTime(saved) }) };
	}
	return { tone: 'off', icon: WaitingIcon, label: t('desktop.folder.state.waitingImages') };
}
