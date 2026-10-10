import { describe, expect, it } from 'vitest';
import { t } from '../../shared/i18n/testing';
import type { BatchCounts, BatchItem, BatchSummary } from '../../shared/ipc/bindings';
import { hasProblem, itemStatus } from './item-status';
import { analysisName, seriesStatus } from './status';

const COUNTS: BatchCounts = {
	files: 2,
	done: 2,
	failed: 0,
	problems: 0,
	active: 0,
	reportsSaved: 1,
	reportsProblem: 0,
	reportsTotal: 1,
	reportsStale: 0,
};

function summary(counts: Partial<BatchCounts>, open = false): BatchSummary {
	return {
		id: '1',
		label: 'S',
		origin: { kind: 'oneOff' },
		open,
		openedAt: '2026-10-12T08:00:00.000Z',
		closedAt: open ? null : '2026-10-12T09:00:00.000Z',
		counts: { ...COUNTS, ...counts },
		needsAttention: false,
		closureReason: null,
	};
}

describe('series status', () => {
	it.each([
		[summary({}, true), 'Collecting images'],
		[summary({ reportsProblem: 1 }), 'Report not saved'],
		[summary({ active: 1 }), 'Analyzing'],
		[summary({ reportsTotal: 0, reportsSaved: 0 }), 'Creating report'],
		[summary({ reportsTotal: 2 }), 'Creating report'],
		[summary({ reportsStale: 1 }), 'Newer results available'],
		[summary({ problems: 1, done: 0, reportsSaved: 0, reportsTotal: 0 }), 'Image not usable'],
		[summary({}), 'Saved'],
		[summary({ done: 0, reportsSaved: 0, reportsTotal: 0 }), 'Done'],
	])('case %#', (series, label) => {
		expect(seriesStatus(series, t).label).toBe(label);
	});

	it('names an analysis in the UI language with fallbacks', () => {
		const analysis = { key: 'g:', modelId: 'grain', names: { en: 'Grain size' } };
		expect(analysisName(analysis, 'de')).toBe('Grain size');
		expect(analysisName({ ...analysis, names: {} }, 'en')).toBe('grain');
		expect(analysisName(undefined, 'en')).toBe('');
	});
});

function item(state: string, computation: string, reason: string | null = null): BatchItem {
	return {
		id: '1',
		label: 'a.tif',
		state,
		reason: null,
		computations: [
			{ id: 'old', modelChoiceKey: 'g:', state: 'superseded', reason: null },
			{ id: 'new', modelChoiceKey: 'g:', state: computation, reason },
		],
	};
}

describe('image status', () => {
	it.each([
		[item('detected', 'queued'), 'Being copied'],
		[item('uploading', 'queued'), 'Uploading'],
		[item('ready', 'queued'), 'Waiting'],
		[item('uploaded', 'queued'), 'Waiting'],
		[item('uploaded', 'computing'), 'Analyzing'],
		[item('uploaded', 'done', 'reused'), 'Done (result taken over)'],
		[item('uploaded', 'done'), 'Done'],
		[item('uploaded', 'failed'), 'Analysis not possible'],
		[item('rejected', 'cancelled'), 'Image not usable'],
		[item('vanished', 'cancelled'), 'Removed'],
		[item('uploaded', 'cancelled'), 'Cancelled'],
		[item('uploaded', 'unavailable'), 'Model no longer available'],
	])('case %#', (entry, label) => {
		expect(itemStatus(entry, 'g:', t).label).toBe(label);
	});

	it('counts file and analysis problems', () => {
		expect(hasProblem(item('uploaded', 'done'))).toBe(false);
		expect(hasProblem(item('uploaded', 'lost'))).toBe(true);
		expect(hasProblem(item('vanished', 'cancelled'))).toBe(true);
	});
});
