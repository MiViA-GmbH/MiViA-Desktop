import { describe, expect, it } from 'vitest';
import { t } from '../../shared/i18n/testing';
import type { BatchSummary, FolderView, HoldKind } from '../../shared/ipc/bindings';
import { folderStatus, monogram } from './status';

const SERIES: BatchSummary = {
	id: '1',
	label: 'S',
	origin: { kind: 'folder', folderId: '1' },
	open: false,
	openedAt: '2026-10-12T07:00:00.000Z',
	closedAt: '2026-10-12T07:12:00.000Z',
	counts: {
		files: 1,
		done: 1,
		failed: 0,
		problems: 0,
		active: 0,
		reportsSaved: 1,
		reportsProblem: 0,
		reportsTotal: 1,
		reportsStale: 0,
	},
	needsAttention: false,
	closureReason: null,
};

const FOLDER: FolderView = {
	id: '1',
	name: 'Lab',
	path: 'D:\\Lab',
	position: 0,
	paused: false,
	missing: false,
	waitingSince: null,
	quietSeconds: 60,
	reportFolder: null,
	openSeries: null,
	lastSeries: null,
	latestReport: null,
};

describe('folder status', () => {
	it.each([
		[{ missing: true, paused: true }, [], 'Folder not found'],
		[{ waitingSince: '2026-10-12T08:30:00.000Z', paused: true }, [], 'Waiting for network folder'],
		[{ paused: true }, ['offline'], 'Paused'],
		[{ openSeries: { ...SERIES, open: true } }, ['offline'], 'Waiting for connection'],
		[{ openSeries: { ...SERIES, open: true } }, ['offline', 'license'], 'Waiting, see the notice above'],
		[{ openSeries: { ...SERIES, open: true } }, [], 'Collecting images'],
		[{ lastSeries: SERIES }, [], 'Report saved 09:12'],
		[{}, [], 'Waiting for new images'],
	])('case %#', (extra, holds, label) => {
		expect(folderStatus({ ...FOLDER, ...extra }, holds as HoldKind[], t).label).toBe(label);
	});
});

describe('monogram', () => {
	it.each([
		['Schliffbilder Mikroskop 1', 'SM'],
		['einschlüsse_labor', 'EL'],
		['Archiv', 'AR'],
		['', ''],
	])('%s', (name, letters) => {
		expect(monogram(name)).toBe(letters);
	});
});
