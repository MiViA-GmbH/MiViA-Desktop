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

async function noHorizontalScroll(page: Page): Promise<void> {
	const overflow = await page.evaluate(
		() => document.documentElement.scrollWidth - document.documentElement.clientWidth,
	);
	expect(overflow).toBeLessThanOrEqual(0);
}

for (const lang of ['en', 'de']) {
	test(`overview with a hold and three problems (${lang})`, async ({ page }) => {
		await open(page, `/?scenario=hold&lang=${lang}`);
		await expect(page.getByRole('status')).toBeVisible();
		await expect(page.locator('.mantine-Badge-root').filter({ hasText: /^3$/ })).toBeVisible();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/o1-overview-hold-${lang}.png` });
	});

	test(`series page (${lang})`, async ({ page }) => {
		await open(page, `/series/214?scenario=calm&lang=${lang}`);
		await expect(page.getByRole('heading', { name: 'Charge 88-214' })).toBeVisible();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/s1-series-${lang}.png` });
	});

	test(`single images dialog (${lang})`, async ({ page }) => {
		await open(page, `/single-images?scenario=one-off&lang=${lang}`);
		const dialog = page.getByRole('dialog');
		await expect(dialog.getByRole('checkbox').first()).toBeChecked();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/d1-one-off-${lang}.png` });
		await dialog.getByRole('button').last().scrollIntoViewIfNeeded();
		await page.screenshot({ path: `${SHOTS}/d2-one-off-footer-${lang}.png` });
	});
}

test('single images page', async ({ page }) => {
	await open(page, '/single-images?scenario=calm');
	await page.screenshot({ path: `${SHOTS}/m1-single-images-en.png` });
	await structure(page);
});

test('calm overview, folder page and settings', async ({ page }) => {
	await open(page, '/?scenario=calm');
	await page.screenshot({ path: `${SHOTS}/o2-overview-calm-en.png` });
	await page
		.getByRole('link', { name: /Schliffbilder Mikroskop 1/ })
		.first()
		.click();
	await page.getByRole('tab', { name: 'Image series' }).waitFor();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/f1-folder-series-en.png` });
	await page
		.getByRole('link', { name: /Settings/ })
		.first()
		.click();
	await expect(page.getByLabel('Language').first()).toBeVisible();
	await page.screenshot({ path: `${SHOTS}/g1-settings-general-en.png` });
	await structure(page);
});

test('empty overview at the minimum window size shows the rail', async ({ page }) => {
	await page.setViewportSize({ width: 760, height: 448 });
	await open(page, '/?scenario=empty');
	await expect(page.getByRole('heading', { name: 'No input folder yet' })).toBeVisible();
	await noHorizontalScroll(page);
	await page.screenshot({ path: `${SHOTS}/z1-min-overview-en.png` });
});

for (const lang of ['en', 'de']) {
	test(`privacy settings, field list and sent data (${lang})`, async ({ page }) => {
		await open(page, `/settings/privacy?scenario=calm&lang=${lang}`);
		const sw = page.getByRole('switch');
		await expect(sw).toBeChecked();
		await expect(page.getByText('5f0c2a71-9e4b-4d2a-8c6f-1b7e3d9a0c42')).toBeVisible();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/g4-settings-privacy-${lang}.png`, fullPage: true });
		await sw.click({ force: true });
		await expect(sw).not.toBeChecked();
		await page.getByRole('button', { name: /^(What is sent|Was gesendet wird)$/ }).click();
		const fields = page.getByRole('dialog');
		await expect(fields.getByText('session.start', { exact: true })).toBeVisible();
		await structure(page);
		await page.screenshot({ path: `${SHOTS}/g5-what-is-sent-${lang}.png` });
		await page.keyboard.press('Escape');
		await page.getByRole('button', { name: /^(View sent data|Gesendete Daten ansehen)$/ }).click();
		const sent = page.getByRole('dialog');
		await sent.getByRole('button').nth(1).click();
		await expect(sent.getByText('folder.paused', { exact: false })).toBeVisible();
		await structure(page);
		await page.screenshot({ path: `${SHOTS}/g6-sent-data-${lang}.png` });
	});

	test(`support settings and the problem report dialog (${lang})`, async ({ page }) => {
		await open(page, `/settings/support?scenario=calm&lang=${lang}`);
		await expect(page.getByRole('row')).toHaveCount(15);
		await expect(page.getByText('0192f1d0')).toBeVisible();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/g3-settings-support-${lang}.png`, fullPage: true });
		await page.getByRole('button', { name: /^(Report a problem|Problem melden)…$/ }).click();
		const dialog = page.getByRole('dialog');
		await dialog.getByRole('button', { name: /^(Review what will be sent|Ansehen, was gesendet wird)$/ }).click();
		await dialog.getByRole('combobox', { name: /^(Show file|Datei anzeigen)$/ }).click();
		await page.getByRole('option', { name: /^system\.json/ }).click();
		await expect(dialog.getByText('"arch":"x86_64"', { exact: false })).toBeVisible();
		await structure(page);
		await page.screenshot({ path: `${SHOTS}/r1-report-dialog-${lang}.png` });
	});
}

test('a policy that sets telemetry locks the switch and hides the device id', async ({ page }) => {
	await open(page, '/settings/privacy?scenario=privacy-locked');
	await expect(page.getByRole('switch')).toBeDisabled();
	await expect(page.getByText('not read')).toBeVisible();
	await expect(page.getByText('Set by your IT department', { exact: false })).toBeVisible();
});

test('sent data shows an empty note when nothing was sent', async ({ page }) => {
	await open(page, '/settings/privacy?scenario=privacy-empty');
	await page.getByRole('button', { name: 'View sent data' }).click();
	await expect(page.getByText('Nothing was sent in the last 7 days.')).toBeVisible();
});

test('a series with problems offers a problem report from its row', async ({ page }) => {
	await open(page, '/history?scenario=calm');
	await page
		.getByRole('row', { name: /Probe 4710/ })
		.getByRole('button', { name: 'More actions' })
		.click();
	await page.getByRole('menuitem', { name: 'Report a problem…' }).click();
	await expect(page.getByRole('dialog', { name: 'Report a problem' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Send', exact: true })).toBeVisible();
});

test('reporting turned off by policy hides the report entry points', async ({ page }) => {
	await open(page, '/settings/support?scenario=reporting-off');
	await expect(page.getByText('Problem reports are turned off by your administrator.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Report a problem…' })).toHaveCount(0);
});
