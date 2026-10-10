import type { TFunction } from 'i18next';

import type { ReportPartView, ReportView } from '../../shared/ipc/bindings';
import {
	DoneIcon,
	ImageProblemIcon,
	ReplacedIcon,
	ReportFailureIcon,
	WaitingIcon,
	WorkingIcon,
} from '../../shared/ui/icons';
import type { Status } from '../../shared/ui/tone';

export type ReportRow = {
	key: string;
	report: ReportView;
	partNo: number;
	partCount: number;
	from: number;
	to: number;
	formats: string[];
	state: string;
	open?: string;
	retry?: string;
};

const RANK: Record<string, number> = { failed: 0, not_saved: 1, requested: 2, saved: 3 };

function worst(parts: ReportPartView[]): ReportPartView | undefined {
	return [...parts].sort((a, b) => (RANK[a.state] ?? 2) - (RANK[b.state] ?? 2))[0];
}

function partRows(report: ReportView): ReportRow[] {
	const numbers = [...new Set(report.parts.map((part) => part.partNo))];
	return numbers.flatMap((partNo) => {
		const parts = report.parts.filter((part) => part.partNo === partNo);
		const first = worst(parts);
		if (!first) {
			return [];
		}
		const pdf =
			parts.find((part) => part.format === 'pdf' && part.state === 'saved') ?? parts.find((p) => p.state === 'saved');
		const failing = parts.find((part) => part.state === 'failed' || part.state === 'not_saved');
		return [
			{
				key: `${report.id}-${partNo}`,
				report,
				partNo,
				partCount: first.partCount,
				from: first.memberFrom + 1,
				to: first.memberTo,
				formats: [...parts].sort((a, b) => b.format.localeCompare(a.format)).map((part) => part.format.toUpperCase()),
				state: first.state,
				...(pdf ? { open: pdf.id } : {}),
				...(failing ? { retry: failing.id } : {}),
			},
		];
	});
}

export type ReportCommand = 'update' | 'keep' | 'move' | 'restore';

export function reportCommands(row: ReportRow): ReportCommand[] {
	const { report } = row;
	if (row.partNo !== 1) {
		return [];
	}
	if (report.moved) {
		return ['restore'];
	}
	if (report.replacedBy) {
		return ['move'];
	}
	return report.stale && row.state === 'saved' ? ['update', 'keep'] : [];
}

export function reportRows(reports: ReportView[]): ReportRow[] {
	return reports.flatMap((report) =>
		report.parts.length > 0
			? partRows(report)
			: [{ key: report.id, report, partNo: 1, partCount: 1, from: 0, to: 0, formats: [], state: report.state }],
	);
}

export function reportStatus(row: ReportRow, t: TFunction): Status {
	if (row.report.moved) {
		return { tone: 'off', icon: ReplacedIcon, label: t('desktop.status.report.replaced') };
	}
	if (row.report.replacedBy) {
		return { tone: 'off', icon: ReplacedIcon, label: t('desktop.status.report.updated') };
	}
	if (row.state === 'failed') {
		return { tone: 'problem', icon: ReportFailureIcon, label: t('desktop.status.report.failed') };
	}
	if (row.state === 'not_saved') {
		return { tone: 'problem', icon: ReportFailureIcon, label: t('desktop.status.report.notSaved') };
	}
	if (row.state === 'saved' && row.report.stale) {
		return { tone: 'waiting', icon: ImageProblemIcon, label: t('desktop.status.report.stale') };
	}
	if (row.state === 'saved') {
		return { tone: 'done', icon: DoneIcon, label: t('desktop.status.report.saved') };
	}
	if (row.state === 'nothing') {
		return { tone: 'off', icon: WaitingIcon, label: t('desktop.status.report.nothing') };
	}
	if (row.state === 'waiting') {
		return { tone: 'waiting', icon: WaitingIcon, label: t('desktop.status.report.waiting') };
	}
	return { tone: 'working', icon: WorkingIcon, label: t('desktop.status.report.creating') };
}
