import type { TFunction } from 'i18next';

import type { BatchItem, ComputationItem } from '../../shared/ipc/bindings';
import { DoneIcon, ImageProblemIcon, ReportFailureIcon, WaitingIcon, WorkingIcon } from '../../shared/ui/icons';
import type { Status } from '../../shared/ui/tone';

const BEFORE_ANALYSIS = new Set(['detected', 'ready', 'uploading']);
const FILE_PROBLEMS = new Set(['rejected', 'unreadable', 'vanished']);
const COMPUTATION_PROBLEMS = new Set(['failed', 'unavailable', 'lost']);

function computationFor(item: BatchItem, key: string): ComputationItem | undefined {
	return item.computations.filter((c) => c.modelChoiceKey === key && c.state !== 'superseded').at(-1);
}

export function hasProblem(item: BatchItem): boolean {
	return FILE_PROBLEMS.has(item.state) || item.computations.some((c) => COMPUTATION_PROBLEMS.has(c.state));
}

function fileStatus(state: string, t: TFunction): Status {
	switch (state) {
		case 'detected':
			return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.sourceFile.detected') };
		case 'uploading':
			return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.sourceFile.uploading') };
		case 'vanished':
			return { tone: 'off', icon: ImageProblemIcon, label: t('desktop.status.sourceFile.vanished') };
		case 'rejected':
		case 'unreadable':
			return { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.sourceFile.rejected') };
		default:
			return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.status.sourceFile.ready') };
	}
}

function computationStatus(computation: ComputationItem, t: TFunction): Status {
	switch (computation.state) {
		case 'done':
			return {
				tone: 'done',
				icon: DoneIcon,
				label:
					computation.reason === 'reused'
						? t('desktop.status.computation.reused')
						: t('desktop.status.computation.done'),
			};
		case 'submitting':
		case 'computing':
			return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.computation.computing') };
		case 'failed':
		case 'lost':
			return { tone: 'waiting', icon: ReportFailureIcon, label: t('desktop.status.computation.failed') };
		case 'unavailable':
			return { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.computation.unavailable') };
		case 'cancelled':
			return { tone: 'off', icon: ImageProblemIcon, label: t('desktop.status.computation.cancelled') };
		default:
			return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.status.computation.queued') };
	}
}

export function itemStatus(item: BatchItem, key: string, t: TFunction): Status {
	const computation = computationFor(item, key);
	if (!computation || BEFORE_ANALYSIS.has(item.state) || FILE_PROBLEMS.has(item.state)) {
		return fileStatus(item.state, t);
	}
	return computationStatus(computation, t);
}
