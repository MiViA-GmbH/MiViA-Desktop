import type {
	BackupsView,
	BatchSummary,
	CustomReportDraft,
	CustomReportsPage,
	HistoryEventsPage,
	HistoryFilter,
	HistoryPage,
	HistoryRow,
	Language,
} from '../../shared/ipc/bindings';
import { ANALYSES, HISTORY_SERIES } from './fixtures';

type Args = Record<string, unknown>;

const PHASE = { key: 'phase:', modelId: 'phase', names: { en: 'Phase fractions', de: 'Phasenanteile' } };

function row(summary: BatchSummary): HistoryRow {
	const index = Number(summary.id) % 3;
	const analyses = index === 0 ? ANALYSES.slice(0, 1) : [...ANALYSES.slice(0, 1), PHASE];
	return {
		summary,
		analyses,
		matched: null,
		savedPart: summary.counts.reportsSaved > 0 ? `${summary.id}1` : null,
	};
}

const IMAGES: Record<string, string> = { '4710': 'Probe 4709 Nachschliff_01.tif' };

function imageMatch(summary: BatchSummary, q: string | undefined): string | undefined {
	const image = IMAGES[summary.id];
	return q && image?.toLowerCase().includes(q) ? image : undefined;
}

function matches(summary: BatchSummary, filter: HistoryFilter): boolean {
	const q = filter.q?.toLowerCase();
	const checks = [
		!q || summary.label.toLowerCase().includes(q) || imageMatch(summary, q) !== undefined,
		!filter.folder || (summary.origin.kind === 'folder' && summary.origin.folderId === filter.folder),
		!filter.manual || summary.origin.kind === 'oneOff',
		filter.status !== 'problems' || summary.counts.problems + summary.counts.reportsProblem > 0,
	];
	return checks.every(Boolean);
}

function page(args: Args): HistoryPage {
	const filter = args.filter as HistoryFilter;
	const q = filter.q?.toLowerCase();
	const rows = HISTORY_SERIES.rows
		.filter((summary) => matches(summary, filter))
		.map((summary) => {
			const image = summary.label.toLowerCase().includes(q ?? '') ? undefined : imageMatch(summary, q);
			return { ...row(summary), matched: image ? { kind: 'image' as const, name: image } : null };
		});
	const searching = Boolean(filter.q || filter.status);
	const head = HISTORY_SERIES.head.filter((summary) => !searching && matches(summary, filter));
	return { revision: '9', head: head.map(row), rows, next: null };
}

const CUSTOM: CustomReportsPage = {
	revision: '9',
	next: null,
	rows: [
		{
			id: '91',
			name: 'Kunde Müller Oktober',
			createdAt: '2026-10-12T07:40:00.000Z',
			images: 24,
			analysis: ANALYSES[0] ?? PHASE,
			state: 'creating',
			stale: false,
			problem: false,
			savedPart: '911',
		},
	],
};

const EVENTS: HistoryEventsPage = {
	revision: '9',
	next: null,
	events: [
		{
			id: '3',
			at: '2026-10-05T08:05:00.000Z',
			tz: 'Europe/Berlin',
			windowsUser: 'LABOR\\moeller',
			action: 'report_saved',
			trigger: 'automatic',
			before: null,
			after: '{"path":"E:\\\\Gusslabor\\\\MiViA reports\\\\2026-10-05 10-02 Charge 88-214 - Grain size.pdf"}',
		},
		{
			id: '2',
			at: '2026-10-05T08:02:00.000Z',
			tz: 'Europe/Berlin',
			windowsUser: 'LABOR\\moeller',
			action: 'series_closed',
			trigger: 'automatic',
			before: null,
			after: '{"reason":"quiet_period"}',
		},
	],
};

const DRAFT: CustomReportDraft = {
	images: 31,
	tooMany: false,
	analyses: [
		{ analysis: ANALYSES[0] ?? PHASE, included: 30, omitted: [{ label: '8214_003.tif', reason: 'FAILED' }], parts: 1 },
		{ analysis: PHASE, included: 31, omitted: [], parts: 1 },
	],
	language: 'en',
	folder: 'D:\\Mikroskopie\\Schliffbilder Mikroskop 1\\MiViA reports',
};

const BACKUPS: BackupsView = {
	folder: '\\\\labserver\\backup\\MiViA',
	lastCopy: '2026-10-12T06:00:00.000Z',
	backups: [
		{ id: 'app:mivia-2026-10-12.db', date: '2026-10-12', inFolder: false, series: 412 },
		{ id: 'folder:mivia-2026-10-12.db', date: '2026-10-12', inFolder: true, series: 412 },
		{ id: 'app:mivia-2026-10-11.db', date: '2026-10-11', inFolder: false, series: 405 },
	],
};

export function historyResponses(scenario: string, language: Language): Record<string, (args: Args) => unknown> {
	const empty = scenario === 'empty';
	return {
		list_history: (args) => (empty ? { revision: '9', head: [], rows: [], next: null } : page(args)),
		list_custom_reports: () => CUSTOM,
		list_batch_events: () => EVENTS,
		get_custom_report_draft: () => ({ ...DRAFT, language }),
		list_backups: () => BACKUPS,
		delete_history: (args) => ({
			images: (args.input as { image: string | null }).image ? 1 : 132,
			filesDeleted: 0,
			filesFailed: 0,
			serverDeleted: 132,
			serverFailed: 0,
		}),
	};
}
