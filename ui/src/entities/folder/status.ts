import type { TFunction } from 'i18next';

import type { FolderView, HoldKind } from '../../shared/ipc/bindings';
import { formatTime } from '../../shared/lib/format';
import { CollectingIcon, DoneIcon, FolderNotFoundIcon, PausedIcon, WaitingIcon } from '../../shared/ui/icons';
import type { Status } from '../../shared/ui/tone';

const NETWORK: HoldKind[] = ['offline', 'serviceUnavailable', 'networkBlocked'];

export function folderStatus(folder: FolderView, holds: HoldKind[], t: TFunction): Status {
	if (folder.missing) {
		return { tone: 'problem', icon: FolderNotFoundIcon, label: t('desktop.folder.state.notFound') };
	}
	if (folder.waitingSince) {
		return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.folder.state.waitingNetwork') };
	}
	if (folder.paused) {
		return { tone: 'off', icon: PausedIcon, label: t('desktop.folder.state.paused') };
	}
	if (holds.length > 0 && folder.openSeries) {
		const network = holds.every((kind) => NETWORK.includes(kind));
		const label = network ? t('desktop.folder.state.waitingConnection') : t('desktop.folder.state.waitingHold');
		return { tone: 'waiting', icon: WaitingIcon, label };
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

export function quietMinutes(folder: FolderView): number {
	return Math.max(1, Math.round(folder.quietSeconds / 60));
}
