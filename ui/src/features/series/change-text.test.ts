import { describe, expect, it } from 'vitest';

import { t } from '../../shared/i18n/testing';
import type { HistoryEventView } from '../../shared/ipc/bindings';
import { eventBy, eventDetail, eventTitle } from './change-text';

function event(action: string, extra: Partial<HistoryEventView> = {}): HistoryEventView {
	return {
		id: '1',
		at: '2026-10-12T08:00:00.000Z',
		tz: 'Europe/Berlin',
		windowsUser: 'LABOR\\moeller',
		action,
		trigger: 'user',
		before: null,
		after: null,
		...extra,
	};
}

describe('change texts', () => {
	it('names known actions and keeps unknown ones as they are', () => {
		expect(eventTitle(event('moved_to_replaced'), t)).toBe('Report moved to Replaced');
		expect(eventTitle(event('something_new'), t)).toBe('something_new');
	});

	it('describes renames, closure reasons, attention and paths', () => {
		const renamed = event('series_renamed', { before: '{"label":"A"}', after: '{"label":"B"}' });
		expect(eventDetail(renamed, t)).toBe('A to B');
		expect(eventDetail(event('series_closed', { after: '{"reason":"manual"}' }), t)).toBe('Finished by hand');
		expect(eventDetail(event('attention_cleared', { after: '{"how":"kept"}' }), t)).toBe('Kept as is');
		expect(eventDetail(event('report_saved', { after: '{"path":"D:\\\\r.pdf"}' }), t)).toBe('D:\\r.pdf');
		expect(eventDetail(event('report_saved', { after: 'not json' }), t)).toBe('');
	});

	it('shows the Windows user for user actions and a source otherwise', () => {
		expect(eventBy(event('series_renamed'), t)).toBe('LABOR\\moeller');
		expect(eventBy(event('series_opened', { trigger: 'send_to' }), t)).toBe('Single images');
	});
});
