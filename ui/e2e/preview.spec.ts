import AxeBuilder from '@axe-core/playwright';
import { expect, type Page, test } from '@playwright/test';

const NOW = new Date('2026-10-12T10:30:00+02:00');
const SHOTS = 'e2e/shots';

async function open(page: Page, path: string): Promise<void> {
	await page.clock.setFixedTime(NOW);
	await page.goto(path);
	await page.locator('h2').first().waitFor();
	await page.evaluate(() => document.fonts.ready);
}

async function structure(page: Page): Promise<void> {
	const result = await new AxeBuilder({ page })
		.withTags(['wcag2a', 'wcag21a'])
		.disableRules(['color-contrast'])
		.analyze();
	expect(result.violations.map((violation) => violation.id)).toEqual([]);
}

const viewer = (page: Page) => page.locator('img[alt="8214_001.tif"]');

for (const lang of ['en', 'de']) {
	test(`preview opens from the eye icon and shows every result type (${lang})`, async ({ page }) => {
		await open(page, `/series/214?scenario=preview&lang=${lang}`);
		await page.getByRole('link', { name: /8214_001\.tif/ }).click();
		await expect(page.getByRole('heading', { name: '8214_001.tif' })).toBeVisible();
		await expect(viewer(page)).toBeVisible();
		await expect(
			page.getByText(lang === 'en' ? 'Average grain size number G' : 'Mittlere Korngrößenzahl G', { exact: true }),
		).toBeVisible();
		await expect(
			page.getByRole('img', { name: /^(Grain size number G according|Korngrößenzahl G nach)/ }),
		).toBeVisible();
		await expect(page.getByText('GRAIN_MULTI_G_AREA_SHOW_MEAN')).toHaveCount(0);
		await expect(page.getByRole('img', { name: 'DECARB_THICKNESS_HIST' })).toBeVisible();
		await expect(page.getByRole('cell', { name: 'ferrite' }).first()).toBeVisible();
		await expect(
			page.getByText(lang === 'en' ? 'This result cannot be shown' : 'Dieses Ergebnis kann nicht angezeigt werden'),
		).toHaveCount(2);
		await expect(page.getByRole('button', { name: /GRAIN_MASK/ })).toContainText(
			lang === 'en' ? 'Preview not available' : 'Vorschau nicht verfügbar',
		);
		await structure(page);
		await page.screenshot({ path: `${SHOTS}/p1-preview-${lang}.png`, fullPage: true });
	});
}

test('zooming swaps to the sharper tier and stops at the largest', async ({ page }) => {
	await open(page, '/preview/214/1?scenario=preview');
	const picture = viewer(page);
	await expect(picture).toBeVisible();
	await expect.poll(async () => decodeURIComponent((await picture.getAttribute('src')) ?? '')).toContain('>2048<');
	const zoomIn = page.getByRole('button', { name: 'Zoom in' });
	for (let step = 0; step < 8 && (await zoomIn.isEnabled()); step += 1) {
		await zoomIn.click();
	}
	await expect(zoomIn).toBeDisabled();
	await expect.poll(async () => decodeURIComponent((await picture.getAttribute('src')) ?? '')).toContain('>4096<');
	await page.getByRole('button', { name: 'Fit to window' }).click();
	await expect(zoomIn).toBeEnabled();
});

test('a result image opens in the viewer and the source comes back', async ({ page }) => {
	await open(page, '/preview/214/1?scenario=preview');
	await page
		.getByRole('button', { name: /placeholder\.png|GRAIN_/ })
		.first()
		.click();
	await expect(page.locator('img[alt="8214_001.tif"]')).toHaveCount(1);
	await page.getByRole('button', { name: 'Show original image' }).click();
	await expect(page.getByRole('button', { name: 'Show original image' })).toHaveCount(0);
});

test('an unreachable server leaves a note and Open in Windows', async ({ page }) => {
	await open(page, '/preview/214/1?scenario=preview-offline');
	await expect(page.getByText('The server could not be reached.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Open in Windows' })).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/p2-preview-offline-en.png` });
});
