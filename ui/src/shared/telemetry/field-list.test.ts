import { describe, expect, it } from 'vitest';

import { loadFieldList } from './field-list';

describe('the generated field lists', () => {
	it('list the same events and fields in English and German', async () => {
		const [en, de] = await Promise.all([loadFieldList('en'), loadFieldList('de-DE')]);
		const shape = (list: typeof en) =>
			list.events.map((event) => [event.name, event.category, event.fields.map((field) => field.name)]);
		expect(shape(de)).toEqual(shape(en));
		expect(en.retentionDays).toBe(de.retentionDays);
	});

	it('describe every event and offer only closed values', async () => {
		const list = await loadFieldList('en');
		expect(list.events.length).toBeGreaterThan(50);
		for (const event of list.events) {
			expect(event.purpose.length).toBeGreaterThan(10);
			expect(event.fields.length).toBeGreaterThan(0);
			for (const field of event.fields) {
				expect(['number', 'flag', 'id', 'version', 'choice']).toContain(field.type);
				expect(field.values !== undefined).toBe(field.type === 'choice');
				for (const value of field.values ?? []) {
					expect(value).toMatch(/^[A-Za-z0-9._:+-]{1,64}$/);
				}
			}
		}
	});

	it('translate the purposes', async () => {
		const [en, de] = await Promise.all([loadFieldList('en'), loadFieldList('de')]);
		const same = en.events.filter((event, index) => event.purpose === de.events[index]?.purpose);
		expect(same.map((event) => event.name)).toEqual([]);
	});
});
