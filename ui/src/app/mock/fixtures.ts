import type {
	BatchCounts,
	BatchItem,
	BatchSummary,
	FolderView,
	HistoryPage,
	HoldsView,
	NeedsAttentionView,
	OneOffView,
	ReportPartView,
	SeriesReportsView,
} from '../../shared/ipc/bindings';

const COUNTS: BatchCounts = {
	files: 0,
	done: 0,
	failed: 0,
	problems: 0,
	active: 0,
	reportsSaved: 0,
	reportsProblem: 0,
	reportsTotal: 0,
	reportsStale: 0,
};

function series(id: string, label: string, folderId: string | null, extra: Partial<BatchSummary>): BatchSummary {
	return {
		id,
		label,
		origin: folderId ? { kind: 'folder', folderId } : { kind: 'oneOff' },
		open: false,
		openedAt: '2026-10-12T06:15:00.000Z',
		closedAt: '2026-10-12T07:12:00.000Z',
		counts: COUNTS,
		needsAttention: false,
		closureReason: 'quiet_period',
		...extra,
	};
}

const saved = (files: number, reports: number): BatchCounts => ({
	...COUNTS,
	files,
	done: files,
	reportsSaved: reports,
	reportsTotal: reports,
});

const OPEN_4711 = series('4711', 'Probe 4711 42CrMo4 Querschliff', '1', {
	open: true,
	closedAt: null,
	counts: { ...COUNTS, files: 12, done: 7, active: 5 },
});

const OPEN_88231 = series('88231', 'Charge 88-231', '2', {
	open: true,
	closedAt: null,
	counts: { ...COUNTS, files: 4, active: 4 },
});

const RECENT: BatchSummary[] = [
	series('4709', 'Probe 4709 42CrMo4 Längsschliff', '1', {
		closedAt: '2026-10-12T07:12:00.000Z',
		counts: saved(18, 2),
	}),
	series('407', 'GJS-400 Ring 7', '3', { closedAt: '2026-10-12T06:47:00.000Z', counts: saved(9, 1) }),
	series('88229', 'Charge 88-229', '2', {
		closedAt: '2026-10-11T14:30:00.000Z',
		counts: { ...saved(40, 0), reportsProblem: 1, reportsTotal: 1 },
		needsAttention: true,
	}),
	series('3', '3 images', null, { closedAt: '2026-10-11T09:05:00.000Z', counts: saved(3, 1) }),
	series('4710', 'Probe 4710', '1', {
		closedAt: '2026-10-05T12:22:00.000Z',
		counts: { ...saved(14, 2), done: 13, failed: 1, problems: 1 },
		needsAttention: true,
	}),
	series('214', 'Charge 88-214', '2', {
		openedAt: '2026-10-05T06:15:00.000Z',
		closedAt: '2026-10-05T08:02:00.000Z',
		counts: { ...saved(132, 2), done: 131, failed: 1, problems: 1, reportsTotal: 4, reportsStale: 2 },
	}),
];

function folder(id: string, path: string, extra: Partial<FolderView>): FolderView {
	return {
		id,
		name: path.split('\\').at(-1) ?? path,
		path,
		position: Number(id),
		paused: false,
		missing: false,
		quietSeconds: 300,
		reportFolder: `${path}\\MiViA reports`,
		openSeries: null,
		lastSeries: null,
		latestReport: null,
		...extra,
	};
}

export const FOLDERS: FolderView[] = [
	folder('1', 'D:\\Mikroskopie\\Schliffbilder Mikroskop 1', {
		openSeries: OPEN_4711,
		lastSeries: RECENT[0] ?? null,
		latestReport: '41',
	}),
	folder('2', '\\\\labserver\\mikroskopie\\Einschlüsse Labor 2', {
		openSeries: OPEN_88231,
		lastSeries: RECENT[2] ?? null,
	}),
	folder('3', 'E:\\Gusslabor\\Gusseisen Graphit', { lastSeries: RECENT[1] ?? null, latestReport: '42' }),
	folder('4', 'D:\\Archiv\\Archiv Altproben', { paused: true }),
	folder('5', 'D:\\Gefuege\\Gefüge Mikroskop 2', { missing: true }),
];

export const HOLDS: HoldsView = { revision: '9', holds: [{ kind: 'offline', since: '2026-10-12T08:42:00.000Z' }] };

export const ATTENTION: NeedsAttentionView = {
	revision: '9',
	total: 3,
	items: [
		{ kind: 'folderNotFound', folderId: '5', folderName: 'Gefüge Mikroskop 2' },
		{ kind: 'reportNotSaved', series: { id: '88229', label: 'Charge 88-229' }, partId: '77' },
		{ kind: 'seriesProblems', series: { id: '4710', label: 'Probe 4710' }, problems: 1 },
	],
};

export const HISTORY: HistoryPage = { revision: '9', head: [OPEN_4711, OPEN_88231], rows: RECENT, next: null };

const SERIES_214 = RECENT[5] ?? OPEN_4711;

const ANALYSES = [
	{ key: 'grain:', modelId: 'grain', names: { en: 'Grain size', de: 'Korngröße' } },
	{ key: 'incl:', modelId: 'incl', names: { en: 'Inclusions', de: 'Einschlüsse' } },
];

export const BATCH = { revision: '9', summary: SERIES_214, analyses: ANALYSES };

function part(id: string, partNo: number, format: string, state: string): ReportPartView {
	const [from, to] = partNo === 1 ? [0, 100] : [100, 132];
	return {
		id,
		partNo,
		partCount: 2,
		format,
		memberFrom: from,
		memberTo: to,
		state,
		reason: null,
		fileName: state === 'saved' ? `Charge 88-214.${format}` : null,
	};
}

export const REPORTS: SeriesReportsView = {
	revision: '9',
	reports: [
		{
			id: '1',
			analysisKey: 'grain:',
			state: 'creating',
			stale: false,
			parts: [
				part('11', 1, 'pdf', 'saved'),
				part('12', 1, 'csv', 'saved'),
				part('13', 2, 'pdf', 'saved'),
				part('14', 2, 'csv', 'saved'),
			],
		},
		{
			id: '2',
			analysisKey: 'incl:',
			state: 'creating',
			stale: true,
			parts: [
				part('21', 1, 'pdf', 'saved'),
				part('22', 1, 'csv', 'saved'),
				part('23', 2, 'pdf', 'requested'),
				part('24', 2, 'csv', 'requested'),
			],
		},
	],
};

function item(index: number, incl: string, reason: string | null = null): BatchItem {
	const id = String(index);
	return {
		id,
		label: `8214_${id.padStart(3, '0')}.tif`,
		state: 'uploaded',
		reason: null,
		computations: [
			{ id: `${id}a`, modelChoiceKey: 'grain:', state: index === 5 ? 'computing' : 'done', reason: null },
			{ id: `${id}b`, modelChoiceKey: 'incl:', state: incl, reason },
		],
	};
}

export const ITEMS = {
	revision: '9',
	next: null,
	items: [item(1, 'done'), item(2, 'done'), item(3, 'failed', 'server_failed'), item(4, 'done'), item(5, 'computing')],
};

export const ONE_OFF: OneOffView = {
	count: 12,
	groups: [
		{
			folder: 'D:\\Labor\\Kunde Müller\\A-2026-0412',
			names: Array.from({ length: 12 }, (_, index) => `Probe Gefüge ${String(index + 1).padStart(4, '0')}.png`),
		},
	],
	notIncluded: [{ name: 'Messprotokoll.docx', reason: 'notImage' }],
	subfolders: 2,
	tooMany: false,
	label: 'Probe Gefüge 0001 +11',
	lastChoices: [{ modelId: 'grain', customizationId: null }],
	lastChosenAt: '2026-10-11T14:20:00.000Z',
	reportFolder: 'D:\\Labor\\Kunde Müller\\A-2026-0412\\MiViA reports',
	reportFallback: false,
	language: 'en',
};
