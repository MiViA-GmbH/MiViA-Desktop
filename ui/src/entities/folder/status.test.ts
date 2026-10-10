import { describe, expect, it } from 'vitest';
import { t } from '../../shared/i18n/testing';
import type { BatchSummary, FolderView } from '../../shared/ipc/bindings';
import { folderStatus } from './status';

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
		[{ missing: true, paused: true }, false, 'Folder not found'],
		[{ waitingSince: '2026-10-12T08:30:00.000Z', paused: true }, false, 'Waiting for network folder'],
		[{ paused: true }, true, 'Paused'],
		[{ openSeries: { ...SERIES, open: true } }, true, 'Waiting for connection'],
		[{ openSeries: { ...SERIES, open: true } }, false, 'Collecting images'],
		[{ lastSeries: SERIES }, false, 'Report saved 09:12'],
		[{}, false, 'Waiting for new images'],
	])('case %#', (extra, offline, label) => {
		expect(folderStatus({ ...FOLDER, ...extra }, offline, t).label).toBe(label);
	});
});
