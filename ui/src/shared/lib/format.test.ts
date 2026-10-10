import { describe, expect, it } from 'vitest';

import { formatDateTime, formatTime, setFormatLocale } from './format';

describe('format', () => {
	it('shows the local time on the same day and the date otherwise, across the DST change', () => {
		setFormatLocale('de-DE');
		const now = new Date('2026-10-25T12:00:00Z');
		expect(formatDateTime('2026-10-25T00:59:59Z', now)).toBe('02:59');
		expect(formatDateTime('2026-10-25T01:00:00Z', now)).toBe('02:00');
		expect(formatDateTime('2026-03-29T00:59:59Z', now)).toBe('29.03., 01:59');
		expect(formatDateTime('2025-12-31T22:59:00Z', now)).toBe('31.12.2025, 23:59');
	});

	it('falls back to the language when the region is unknown to Intl', () => {
		setFormatLocale('en-ZZ');
		expect(formatTime('2026-10-12T08:42:00Z')).toMatch(/10:42/);
	});
});
