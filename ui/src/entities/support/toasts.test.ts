import i18n from 'i18next';
import { beforeAll, describe, expect, it } from 'vitest';

import en from '../../shared/i18n/locales/desktop/en.json';
import type { AppError } from '../../shared/ipc/bindings';
import { failureMessage, outcomeMessage } from './toasts';

function appError(code: string | null, fields: Record<string, string> | null = null): AppError {
	return {
		kind: 'validation',
		code,
		message: 'raw text',
		signature: `validation:${code ?? '-'}:site`,
		reportable: false,
		traceId: null,
		computationId: null,
		fields,
	};
}

beforeAll(async () => {
	await i18n.init({ lng: 'en', resources: { en: { translation: en } }, interpolation: { escapeValue: false } });
});

describe('support toasts', () => {
	it('names the reference for every send outcome', () => {
		expect(outcomeMessage({ kind: 'sent', reference: '0192abcd' })).toBe('Problem report sent. Reference 0192abcd.');
		expect(outcomeMessage({ kind: 'queued', reference: '0192abcd' })).toContain('0192abcd is sent automatically');
		expect(outcomeMessage({ kind: 'saveAndEmail', reference: '0192abcd' })).toContain('e-mail it to support');
	});

	it('maps report error codes and never shows the raw message', () => {
		expect(failureMessage(appError('reportingOff'))).toBe('Problem reports are turned off by your administrator.');
		expect(failureMessage(appError('reportExpired'))).toContain('no longer available');
		expect(failureMessage(appError('somethingNew'))).toBe('That did not work. Try again in a moment.');
		expect(failureMessage(new Error('boom'))).toBe('That did not work. Try again in a moment.');
	});

	it('names the report file that was moved, deleted or changed', () => {
		expect(failureMessage(appError('reportMissing', { name: 'Charge 88-214.pdf' }))).toBe(
			'The report “Charge 88-214.pdf” was moved or deleted outside MiViA Desktop.',
		);
		expect(failureMessage(appError('reportChanged', { name: 'a.csv' }))).toContain('“a.csv” was changed');
		expect(failureMessage(appError('reportMissing'))).toContain('This report file was moved');
	});
});
