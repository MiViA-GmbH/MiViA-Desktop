import { describe, expect, it } from 'vitest';

import { clampOffset, fitSize, maxZoom, needsView, stepZoom, tierWidth, zoomAbout } from './viewport';

const big = { width: 8000, height: 6000 };

describe('tiers', () => {
	it('measures the tier width from the long edge', () => {
		expect(tierWidth(big, 2048)).toBe(2048);
		expect(tierWidth(big, 4096)).toBe(4096);
		expect(tierWidth({ width: 1000, height: 4000 }, 2048)).toBe(512);
		expect(tierWidth({ width: 800, height: 600 }, 2048)).toBe(800);
	});

	it('asks for the view tier once the display tier is stretched past a quarter', () => {
		expect(needsView(1000, 1, 1, big)).toBe(false);
		expect(needsView(1000, 2.5, 1, big)).toBe(false);
		expect(needsView(1000, 2.6, 1, big)).toBe(true);
		expect(needsView(1500, 1, 2, big)).toBe(true);
	});
});

describe('zoom', () => {
	it('stops at the largest tier plus a quarter', () => {
		expect(maxZoom(1000, 1, big, true)).toBeCloseTo(5.12);
		expect(maxZoom(1000, 1, big, false)).toBeCloseTo(2.56);
	});

	it('never goes below the fitted size, even for small images', () => {
		expect(maxZoom(1000, 1, { width: 400, height: 300 }, false)).toBe(1);
	});

	it('steps within bounds', () => {
		expect(stepZoom(1, -1, 4)).toBe(1);
		expect(stepZoom(3, 1, 4)).toBe(4);
		expect(stepZoom(2, 1, 4)).toBe(3);
	});

	it('keeps the point under the cursor fixed', () => {
		expect(zoomAbout(100, 0, 1, 2)).toBe(-100);
		expect(zoomAbout(0, 0, 1, 2)).toBe(0);
	});
});

describe('placing', () => {
	it('centres content that fits and keeps larger content covering the container', () => {
		expect(clampOffset(50, 400, 1000)).toBe(300);
		expect(clampOffset(50, 2000, 1000)).toBe(0);
		expect(clampOffset(-1500, 2000, 1000)).toBe(-1000);
	});

	it('fits inside the container', () => {
		expect(fitSize({ width: 1000, height: 500 }, { width: 400, height: 400 })).toEqual({ width: 500, height: 500 });
	});
});
