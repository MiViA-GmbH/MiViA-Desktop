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

for (const lang of ['en', 'de']) {
	test(`network block, waiting folder and a folder watched twice (${lang})`, async ({ page, context }) => {
		await context.grantPermissions(['clipboard-read', 'clipboard-write']);
		await open(page, `/?scenario=network&lang=${lang}`);
		const banner = page.getByRole('status').filter({ hasText: 'Contoso Inspection CA' });
		await expect(banner).toBeVisible();
		await banner.getByRole('button').first().click();
		await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('Host: app.mivia.ai');
		await expect(page.getByText(/LAB-PC-2/)).toBeVisible();
		await expect(page.getByText(/labserver/)).toBeVisible();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/o3-overview-network-${lang}.png` });
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
		await page.getByRole('button', { name: /^(Administrator policies|Richtlinien des Administrators)$/ }).click();
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

for (const lang of ['en', 'de']) {
	test(`update settings with a ready update (${lang})`, async ({ page }) => {
		await open(page, `/settings/updates?scenario=update-ready&lang=${lang}`);
		await expect(page.getByText(/2\.1\.0 is ready|2\.1\.0 ist bereit/)).toBeVisible();
		await expect(
			page.getByRole('button', { name: /^(Restart and update|Neu starten und aktualisieren)$/ }),
		).toBeVisible();
		await expect(page.getByRole('button', { name: /^(Report a problem|Problem melden)$/ })).toBeVisible();
		await structure(page);
		await noHorizontalScroll(page);
		await page.screenshot({ path: `${SHOTS}/g7-settings-updates-${lang}.png`, fullPage: true });
	});
}

test('a policy that turns off automatic updates locks the switch', async ({ page }) => {
	await open(page, '/settings/updates?scenario=update-locked');
	const sw = page.getByRole('switch', { name: 'Download and install updates automatically' });
	await expect(sw).toBeDisabled();
	await expect(sw).not.toBeChecked();
	await expect(page.getByText('Set by your IT department', { exact: false })).toBeVisible();
});

test('a beta build keeps the channel on Beta and cannot pick Stable', async ({ page }) => {
	await open(page, '/settings/updates?scenario=update-beta-build');
	await page.getByRole('button', { name: 'Advanced' }).click();
	await expect(page.getByText('Beta builds always use the Beta channel', { exact: false })).toBeVisible();
	await page.getByRole('combobox', { name: 'Update channel' }).click();
	await expect(page.getByRole('option', { name: 'Stable' })).toHaveAttribute('data-combobox-disabled', 'true');
});

test('a possible update marks Settings and offers to show the Updates page', async ({ page }) => {
	await open(page, '/?scenario=update-notice-available');
	await expect(page.getByRole('navigation').getByRole('img', { name: 'Update available' })).toBeVisible();
	const notice = page.getByRole('status').filter({ has: page.getByRole('button', { name: 'Show' }) });
	await expect(notice).toContainText('Version 2.1.0 is available.');
	await notice.getByRole('button', { name: 'Show' }).click();
	await expect(page).toHaveURL(/\/settings\/updates/);
	await expect(page.getByRole('img', { name: 'Update available' })).toHaveCount(2);
	await expect(notice).toBeHidden();
});

test('a downloaded update is announced as ready to install', async ({ page }) => {
	await open(page, '/?scenario=update-notice-ready');
	const notice = page.getByRole('status').filter({ hasText: 'Version 2.1.0 is ready to install.' });
	await expect(notice.getByRole('button', { name: 'Show' })).toBeVisible();
});

test('a required update shows a red banner with one filled restart button', async ({ page }) => {
	await open(page, '/?scenario=too-old');
	const banner = page.getByRole('status').filter({ hasText: 'Update required.' });
	await expect(banner).toBeVisible();
	await expect(banner.getByRole('button')).toHaveCount(1);
	const fill = await banner
		.getByRole('button', { name: 'Restart and update' })
		.evaluate((button) => getComputedStyle(button).backgroundColor);
	expect(fill).not.toMatch(/^rgba\(0, 0, 0, 0\)$|^rgb\(255, 255, 255\)$/);
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/o4-update-required-en.png` });
});

test('a required version without a signed update shows a warning banner', async ({ page }) => {
	await open(page, '/?scenario=update-unsigned');
	const banner = page.getByRole('status').filter({ hasText: 'A newer version is required.' });
	await expect(banner.getByRole('button', { name: 'Download page' })).toBeVisible();
	await expect(banner.getByRole('button', { name: 'Report a problem' })).toBeVisible();
	await structure(page);
	await noHorizontalScroll(page);
	await page.screenshot({ path: `${SHOTS}/o5-update-unsigned-en.png` });
});

test('the access key hold opens the account panel where signing in again works', async ({ page }) => {
	await open(page, '/?scenario=hold-accessKey');
	await page.getByRole('button', { name: 'Sign in again' }).click();
	await expect(page.getByRole('tab', { name: 'Browser' })).toHaveAttribute('aria-selected', 'true');
	await expect(page.getByRole('tab', { name: 'Code' })).toBeVisible();
	await expect(page.getByRole('tab', { name: 'API key' })).toBeVisible();
	await structure(page);
	await page.getByRole('button', { name: 'Open the browser' }).click();
	await expect(page.getByText('New API key saved.')).toBeVisible();
});

test('the account panel offers the tabs without an account and still takes a pasted key', async ({ page }) => {
	await open(page, '/settings/account?scenario=no-account');
	await expect(page.getByRole('tab', { name: 'Browser' })).toHaveAttribute('aria-selected', 'true');
	await page.getByRole('tab', { name: 'API key' }).click();
	await expect(page.getByText('No account is connected.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'app.mivia.ai/api-keys' })).toBeVisible();
	const field = page.getByRole('tabpanel').getByLabel('API key');
	await field.fill('mv-AbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKl');
	await page.getByRole('button', { name: 'Save key' }).click();
	await expect(page.getByText('New API key saved.')).toBeVisible();
});
