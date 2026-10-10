import type { TFunction } from 'i18next';

import type { AnalysisView, BatchSummary, Language } from '../../shared/ipc/bindings';
import { CollectingIcon, DoneIcon, ImageProblemIcon, ReportFailureIcon, WorkingIcon } from '../../shared/ui/icons';
import type { Status } from '../../shared/ui/tone';

export function seriesStatus(series: BatchSummary, t: TFunction): Status {
	const { counts } = series;
	if (series.open) {
		return { tone: 'working', icon: CollectingIcon, label: t('desktop.status.batch.collecting') };
	}
	if (counts.reportsProblem > 0) {
		return { tone: 'problem', icon: ReportFailureIcon, label: t('desktop.status.batch.reportProblem') };
	}
	if (counts.active > 0) {
		return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.batch.analyzing') };
	}
	const reportsPending = counts.reportsTotal === 0 || counts.reportsSaved < counts.reportsTotal;
	if (counts.done > 0 && reportsPending) {
		return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.batch.creatingReport') };
	}
	if (counts.reportsStale > 0) {
		return { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.batch.newerResults') };
	}
	if (counts.problems > 0) {
		return { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.batch.imageProblems') };
	}
	if (counts.reportsSaved > 0) {
		return { tone: 'done', icon: DoneIcon, label: t('desktop.status.batch.saved') };
	}
	return { tone: 'done', icon: DoneIcon, label: t('desktop.status.batch.done') };
}

export function analysisName(analysis: AnalysisView | undefined, language: Language): string {
	return analysis?.names[language] ?? analysis?.names.en ?? analysis?.modelId ?? '';
}
