import { describe, expect, it } from 'vitest';

import { groupItems, ofType, parseResults, roundValue, xText } from './results';

describe('parseResults', () => {
	it('keeps objects and drops everything else', () => {
		expect(parseResults('[{"type":"message","label":""},3,null]')).toEqual([{ type: 'message', label: '' }]);
	});

	it('gives nothing for missing or broken input', () => {
		expect(parseResults(null)).toEqual([]);
		expect(parseResults('{')).toEqual([]);
		expect(parseResults('{"type":"image"}')).toEqual([]);
	});
});

describe('ofType', () => {
	it('selects by type', () => {
		const results = parseResults('[{"type":"image","filename":"a.png"},{"type":"raw-feature"}]');
		expect(ofType(results, 'image')).toHaveLength(1);
	});
});

describe('values', () => {
	it('rounds to two places without trailing zeros', () => {
		expect(roundValue(19.405757)).toBe('19.41');
		expect(roundValue(3)).toBe('3');
		expect(roundValue('<1')).toBe('<1');
	});

	it('writes ranges', () => {
		expect(xText([1.04, 2.5])).toBe('1.04-2.5');
	});

	it('groups in order of first appearance', () => {
		const groups = groupItems([{ group: 'b' }, {}, { group: 'b' }]);
		expect(groups.map((g) => [g.group, g.items.length])).toEqual([
			['b', 2],
			[undefined, 1],
		]);
	});
});
