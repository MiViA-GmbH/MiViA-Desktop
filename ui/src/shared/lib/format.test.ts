import { describe, expect, it } from 'vitest';

import { formatDateTime, formatFullDateTime, formatSize, formatTime, setFormatLocale } from './format';

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

	it('shows sizes in kB below one MB and in MB above', () => {
		setFormatLocale('de-DE');
		expect(formatSize(1536)).toBe('1,5 kB');
		expect(formatSize(10 * 1024 * 1024)).toBe('10 MB');
		setFormatLocale('en-US');
		expect(formatSize(300)).toBe('0.3 kB');
	});

	it('always shows the full date for a period', () => {
		setFormatLocale('de-DE');
		expect(formatFullDateTime('2026-10-12T08:30:00Z')).toBe('12.10.2026, 10:30');
	});
});
