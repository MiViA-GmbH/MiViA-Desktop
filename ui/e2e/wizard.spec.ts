import AxeBuilder from '@axe-core/playwright';
import { expect, type Locator, type Page, test } from '@playwright/test';

const SHOTS = 'e2e/shots';
const KEY = 'mv-AbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKl';

async function open(page: Page, query: string): Promise<void> {
	await page.goto(`/?${query}`);
	await page.locator('h3').first().waitFor();
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

async function tabTo(page: Page, target: Locator): Promise<void> {
	for (let step = 0; step < 40; step += 1) {
		await page.keyboard.press('Tab');
		const focused = await target.evaluate((element) => element === document.activeElement).catch(() => false);
		if (focused) {
			const outline = await target.evaluate((element) => getComputedStyle(element).outlineStyle);
			expect(outline, 'visible focus').not.toBe('none');
			return;
		}
	}
	throw new Error(`${target.toString()} is not reachable with Tab`);
}

const button = (page: Page, name: string) => page.getByRole('button', { name, exact: true });

async function heading(page: Page, name: string): Promise<void> {
	await expect(page.getByRole('heading', { name, level: 3 })).toBeVisible();
	await expect(page.getByRole('heading', { name, level: 3 })).toBeFocused();
}

test('the setup can be finished with the keyboard alone', async ({ page }) => {
	await open(page, 'scenario=setup');
	await heading(page, 'Welcome to MiViA Desktop');
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w01-welcome-en.png` });
	await tabTo(page, button(page, 'Start setup'));
	await page.keyboard.press('Enter');

	await heading(page, 'Connect your MiViA account');
	await expect(page.getByRole('tab', { name: 'Browser' })).toHaveAttribute('aria-selected', 'true');
	await expect(button(page, 'Next')).toBeDisabled();
	await page.keyboard.press('Tab');
	await page.keyboard.press('ArrowRight');
	await expect(page.getByRole('tab', { name: 'API key' })).toHaveAttribute('aria-selected', 'true');
	await page.keyboard.press('Tab');
	await page.keyboard.type('mv-bad-key-0123456789-0123456789-0123456789');
	await page.keyboard.press('Enter');
	await expect(page.getByText('This API key was not accepted.')).toBeVisible();
	await expect(page.getByRole('tabpanel').getByLabel('API key')).toBeFocused();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w03-key-rejected-en.png` });
	await page.keyboard.press('Control+A');
	await page.keyboard.type(KEY);
	await page.keyboard.press('Enter');
	await expect(page.getByText('Connected as Dr. Katrin Möller')).toBeVisible();
	await page.screenshot({ path: `${SHOTS}/w05-key-connected-en.png` });
	await tabTo(page, button(page, 'Next'));
	await page.keyboard.press('Enter');

	await heading(page, 'Where does your microscope save images?');
	await tabTo(page, button(page, 'Next'));
	await page.keyboard.press('Enter');
	await expect(page.getByText('Choose a folder, or skip this step.')).toBeVisible();
	await page.screenshot({ path: `${SHOTS}/w06-folder-empty-error-en.png` });
	await expect(page.getByRole('button', { name: 'Choose folder…' })).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(page.getByText('This folder already contains 42 images.')).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w07-folder-chosen-en.png` });
	await tabTo(page, button(page, 'Next'));
	await page.keyboard.press('Enter');

	await heading(page, 'How do images arrive in this folder?');
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w08-series-en.png` });
	await tabTo(page, button(page, 'Next'));
	await page.keyboard.press('Enter');

	await heading(page, 'Which models should analyze the images?');
	await tabTo(page, button(page, 'Next'));
	await page.keyboard.press('Enter');
	await expect(page.getByText('Choose at least one model.')).toBeVisible();
	await tabTo(page, page.getByRole('checkbox', { name: 'Grain size' }));
	await page.keyboard.press('Space');
	await expect(page.getByRole('combobox', { name: 'Customization' })).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w10-models-en.png` });
	await tabTo(page, button(page, 'Next'));
	await page.keyboard.press('Enter');

	await heading(page, 'Windows and privacy');
	await expect(page.getByRole('checkbox', { name: /Help improve MiViA/ })).not.toBeChecked();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w12-finish-en.png` });
	const more = page.getByRole('button', { name: /^More about Help improve MiViA/ });
	await tabTo(page, more);
	await page.keyboard.press('Enter');
	await expect(page.getByText('You can withdraw your consent at any time in Settings > Privacy.')).toBeVisible();
	await expect(page.getByRole('checkbox', { name: /Help improve MiViA/ })).not.toBeChecked();
	await expect(page.getByRole('dialog')).toHaveCSS('opacity', '1');
	await page.screenshot({ path: `${SHOTS}/w12-finish-help-en.png` });
	await page.keyboard.press('Escape');
	await expect(page.getByText('You can withdraw your consent at any time in Settings > Privacy.')).toBeHidden();
	await expect(more).toBeFocused();
	await tabTo(page, page.getByRole('button', { name: 'What is sent' }));
	await page.keyboard.press('Enter');
	await expect(page.getByRole('dialog').getByText('session.start', { exact: true })).toBeVisible();
	await page.keyboard.press('Escape');
	await tabTo(page, button(page, 'Finish setup'));
	await page.keyboard.press('Enter');

	await heading(page, 'MiViA Desktop is ready');
	await expect(page.getByText('Models: Grain size')).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w13-done-en.png` });
	await tabTo(page, button(page, 'Go to overview'));
	await page.keyboard.press('Enter');
	await expect(page.getByRole('heading', { name: 'Overview', level: 2 })).toBeVisible();
});

test('the account step signs in through the browser and links to the API keys page', async ({ page }) => {
	await open(page, 'scenario=setup');
	await page.getByRole('button', { name: 'Start setup' }).click();
	await heading(page, 'Connect your MiViA account');
	await expect(page.getByRole('tab', { name: 'Code' })).toBeVisible();
	await page.getByRole('tab', { name: 'API key' }).click();
	await expect(page.getByRole('button', { name: 'app.mivia.ai/api-keys' })).toBeVisible();
	await structure(page);
	await page.getByRole('tab', { name: 'Browser' }).click();
	await structure(page);
	await button(page, 'Open the browser').click();
	await expect(page.getByText('Connected as Dr. Katrin Möller')).toBeVisible();
	await expect(button(page, 'Next')).toBeEnabled();
});

test('the old version step offers to take over the old settings', async ({ page }) => {
	await open(page, 'scenario=setup-legacy');
	await page.getByRole('button', { name: 'Start setup' }).click();
	await heading(page, 'Take over your old settings?');
	await expect(page.getByText('C:\\Users\\moeller\\Downloads\\MiViA')).toBeVisible();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w02-legacy-en.png` });
	await page.getByRole('button', { name: 'Take over all' }).click();
	await expect(page.getByText('The API key of the old MiViA Desktop was accepted.')).toBeVisible();
});

test('closing the window during setup asks before quitting', async ({ page }) => {
	await open(page, 'scenario=setup-resume');
	await expect(page.getByText('Setup is not finished yet.')).toBeVisible();
	await page.screenshot({ path: `${SHOTS}/w16-resume-en.png` });
	await page.evaluate(() => {
		type Internals = { invoke: (command: string, args: Record<string, unknown>) => Promise<unknown> };
		const internals = (window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__;
		return internals.invoke('plugin:event|emit', { event: 'quit-requested', payload: null });
	});
	await expect(page.getByRole('dialog', { name: 'Finish setup later?' })).toBeVisible();
	await expect(page.getByRole('dialog').getByRole('button', { name: 'Continue setup' }).last()).toBeFocused();
	await structure(page);
	await page.screenshot({ path: `${SHOTS}/w15-quit-en.png` });
	await page.keyboard.press('Escape');
	await expect(page.getByRole('dialog')).toBeHidden();
});

test('the German setup fits and the window minimum hides the step list', async ({ page }) => {
	await open(page, 'scenario=setup&lang=de');
	await page.getByRole('button', { name: 'Einrichtung starten' }).click();
	await page.getByRole('tab', { name: 'API-Schlüssel' }).click();
	await page.getByRole('tabpanel').getByLabel('API-Schlüssel').fill(KEY);
	await page.getByRole('button', { name: 'Weiter' }).click();
	await page.getByRole('button', { name: 'Weiter' }).click();
	await page.getByRole('button', { name: 'Ordner wählen…' }).click();
	await page.getByRole('button', { name: 'Weiter' }).click();
	await heading(page, 'Wie kommen die Bilder in diesen Ordner?');
	await noHorizontalScroll(page);
	await page.screenshot({ path: `${SHOTS}/w17-series-de.png` });
	await page.setViewportSize({ width: 760, height: 448 });
	await expect(page.getByText('Schritt 3 von 5')).toBeVisible();
	await noHorizontalScroll(page);
	await page.screenshot({ path: `${SHOTS}/z2-min-wizard-de.png` });
});
