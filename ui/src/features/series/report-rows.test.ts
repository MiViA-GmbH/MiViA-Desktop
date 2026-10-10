import { describe, expect, it } from 'vitest';
import { t } from '../../shared/i18n/testing';
import type { ReportPartView, ReportView } from '../../shared/ipc/bindings';
import { reportCommands, reportRows, reportStatus } from './report-rows';

function part(id: string, partNo: number, format: string, state: string): ReportPartView {
	return {
		id,
		partNo,
		partCount: 2,
		format,
		memberFrom: (partNo - 1) * 100,
		memberTo: partNo * 100,
		state,
		reason: null,
		fileName: null,
	};
}

const report = (parts: ReportPartView[], extra: Partial<ReportView> = {}): ReportView => ({
	id: 'r',
	analysisKey: 'grain:',
	state: 'creating',
	stale: false,
	replacedBy: null,
	moved: false,
	parts,
	...extra,
});

describe('report rows', () => {
	it('joins the formats of one part and shows its worst state', () => {
		const rows = reportRows([
			report([
				part('1', 1, 'csv', 'saved'),
				part('2', 1, 'pdf', 'saved'),
				part('3', 2, 'pdf', 'not_saved'),
				part('4', 2, 'csv', 'saved'),
			]),
		]);
		expect(rows.map((row) => [row.partNo, row.from, row.to, row.formats.join('+'), row.state])).toEqual([
			[1, 1, 100, 'PDF+CSV', 'saved'],
			[2, 101, 200, 'PDF+CSV', 'not_saved'],
		]);
		expect(rows[0]?.open).toBe('2');
		expect(rows[1]?.retry).toBe('3');
		const failing = rows[1];
		expect(failing && reportStatus(failing, t).label).toBe('Report not saved');
	});

	it('shows a report without parts by its own state and a stale saved part as newer results', () => {
		const [waiting] = reportRows([report([], { state: 'waiting' })]);
		expect(waiting && reportStatus(waiting, t).label).toBe('Waiting');
		const [stale] = reportRows([report([part('1', 1, 'pdf', 'saved')], { stale: true })]);
		expect(stale && reportStatus(stale, t).tone).toBe('waiting');
		const [nothing] = reportRows([report([], { state: 'nothing' })]);
		expect(nothing && reportStatus(nothing, t).label).toBe('No report');
		const [failed] = reportRows([report([part('1', 1, 'pdf', 'failed')])]);
		expect(failed && reportStatus(failed, t).label).toBe('Report could not be created');
		const [creating] = reportRows([report([part('1', 1, 'pdf', 'requested')])]);
		expect(creating && reportStatus(creating, t).label).toBe('Creating report');
	});
});

describe('report commands', () => {
	it('offers update and keep on a saved stale report and moves after an update', () => {
		const saved = [part('1', 1, 'pdf', 'saved'), part('2', 2, 'pdf', 'saved')];
		const stale = reportRows([report(saved, { stale: true })]);
		expect(stale.map(reportCommands)).toEqual([['update', 'keep'], []]);
		const replaced = reportRows([report(saved, { stale: true, replacedBy: 's' })]);
		expect(replaced.map(reportCommands)).toEqual([['move'], []]);
		const moved = reportRows([report(saved, { stale: true, replacedBy: 's', moved: true })]);
		expect(moved.map((row) => reportStatus(row, t).label)).toEqual(['Replaced', 'Replaced']);
		expect(moved.map(reportCommands)).toEqual([['restore'], []]);
	});
});
