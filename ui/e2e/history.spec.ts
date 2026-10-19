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
	test(`history list (${lang})`, async ({ page }) => {
		await open(page, `/history?scenario=calm&lang=${lang}`);
		await expect(page.getByRole('row')).toHaveCount(9);
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/y1-history-${lang}.png` });
	});
}

test('history search shows the matched image', async ({ page }) => {
	await open(page, '/history?scenario=calm&q=4709');
	await expect(page.getByText('matched image Probe 4709 Nachschliff_01.tif')).toBeVisible();
	await page.screenshot({ path: `${SHOTS}/y2-history-search-en.png` });
});

test('empty history and a filter without hits', async ({ page }) => {
	await open(page, '/history?scenario=empty');
	await expect(page.getByRole('heading', { name: 'No image series yet' })).toBeVisible();
	await page.screenshot({ path: `${SHOTS}/y3-history-empty-en.png` });
	await open(page, '/history?scenario=calm&q=Probe%209999&status=problems');
	await expect(page.getByRole('heading', { name: 'Nothing found' })).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/y4-history-filtered-empty-en.png` });
	await page.getByRole('button', { name: 'Clear filters' }).click();
	await expect(page).toHaveURL(/\/history$/);
});

test('selected series open the custom report dialog', async ({ page }) => {
	await open(page, '/history?scenario=calm');
	await page.getByRole('checkbox', { name: /Charge 88-214/ }).check();
	await page.getByRole('checkbox', { name: /Probe 4710/ }).check();
	await expect(page.getByText('2 image series selected')).toBeVisible();
	await page.getByRole('button', { name: 'Create custom report' }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog.getByText('This creates 2 reports.')).toBeVisible();
	await dialog.getByRole('button', { name: 'Create report' }).click();
	await expect(dialog.getByText('Enter a name for the report.')).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/c1-custom-report-en.png` });
});

test('the header checkbox selects and clears all shown series', async ({ page }) => {
	await open(page, '/history?scenario=calm');
	const all = page.getByRole('checkbox', { name: 'Select all shown series' });
	await page.getByRole('checkbox', { name: /Charge 88-214/ }).check();
	await expect(all).toHaveAttribute('data-indeterminate', 'true');
	await all.click();
	const rows = await page.getByRole('checkbox', { name: /^Select (?!all)/ }).count();
	await expect(page.getByText(`${rows} image series selected`)).toBeVisible();
	await all.click();
	await expect(page.getByText(/image series selected/)).toHaveCount(0);
});

test('custom reports view', async ({ page }) => {
	await open(page, '/history?scenario=calm&view=custom');
	await expect(page.getByText('Kunde Müller Oktober')).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/c2-custom-reports-en.png` });
});

test('series page lists its changes', async ({ page }) => {
	await open(page, '/series/214?scenario=calm');
	await expect(page.getByRole('heading', { name: 'Changes' })).toBeVisible();
	await expect(page.getByText('No new images arrived for a while')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Update report' })).toBeVisible();
	await page.getByRole('heading', { name: 'Changes' }).scrollIntoViewIfNeeded();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/s2-series-changes-en.png` });
});

test('save results as CSV opens the report dialog with only the data file', async ({ page }) => {
	await open(page, '/series/214?scenario=calm');
	await page.getByRole('button', { name: 'More actions' }).click();
	await page.getByRole('menuitem', { name: 'Save results as CSV' }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog.getByRole('checkbox', { name: 'Data (CSV)' })).toBeChecked();
	await expect(dialog.getByRole('checkbox', { name: 'PDF' })).not.toBeChecked();
});

test('deleting a series asks first and says what each choice does', async ({ page }) => {
	await open(page, '/series/214?scenario=calm');
	await page.getByRole('button', { name: 'More actions' }).click();
	await page.getByRole('menuitem', { name: 'Delete image series' }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog.getByRole('checkbox', { name: /saved report files/ })).not.toBeChecked();
	await expect(dialog.getByRole('checkbox', { name: /MiViA server/ })).toBeChecked();
	await expect(dialog.getByText(/stay in your MiViA account/)).toBeVisible();
	await expect(page.getByRole('menu')).toBeHidden();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/d1-delete-series-en.png` });
	await dialog.getByRole('button', { name: 'Delete' }).click();
	await expect(page.getByText('132 images were removed from the history.')).toBeVisible();
	await expect(page).toHaveURL(/\/history/);
});

test('a failed deletion keeps the dialog open and says why', async ({ page }) => {
	await open(page, '/series/214?scenario=failure');
	await page
		.getByRole('button', { name: /^Delete / })
		.first()
		.click();
	const dialog = page.getByRole('dialog', { name: 'Delete image' });
	await dialog.getByRole('button', { name: 'Delete' }).click();
	await expect(dialog.getByText(/Probe 4709/)).toBeVisible();
	await expect(dialog).toBeVisible();
});

test('an image can be deleted from its series by keyboard', async ({ page }) => {
	await open(page, '/series/214?scenario=calm');
	const remove = page.getByRole('button', { name: /^Delete / }).first();
	await remove.focus();
	await page.keyboard.press('Enter');
	const dialog = page.getByRole('dialog', { name: 'Delete image' });
	await expect(dialog).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(dialog).toBeHidden();
	await expect(remove).toBeFocused();
});

test('settings list backups to restore', async ({ page }) => {
	await open(page, '/settings/general?scenario=calm');
	await expect(page.getByText('\\\\labserver\\backup\\MiViA')).toBeVisible();
	await page.getByRole('button', { name: 'Restore history from backup' }).click();
	await expect(page.getByRole('radio')).toHaveCount(3);
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/g2-restore-backup-en.png` });
});
