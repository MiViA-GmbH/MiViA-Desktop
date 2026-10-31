import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, type Page, test } from '@playwright/test';
import { launch, REPO, type Running, shutdown } from './stub-app';

const RUN = join(REPO, 'target', 'e2e-run', `sign-in-${Date.now()}`);
const COMPUTER = 'LAB-PC-7';
const UNFINISHED = join(RUN, 'unfinished');
const UNFINISHED_WATCHED = join(UNFINISHED, 'Labor');
const UNFINISHED_URL_FILE = join(UNFINISHED, 'sign-in-url.txt');

function signInUrl(): string {
	try {
		return readFileSync(UNFINISHED_URL_FILE, 'utf8');
	} catch {
		return '';
	}
}

test.describe
	.serial('browser sign-in that does not finish', () => {
		let running: Running;

		const answerClientLookup = (answer: 'normal' | 'missing' | 'html' | 'proxyauth') =>
			fetch(`${running.origin}/__stub/by-app`, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(answer),
			});

		const openBrowser = (page: Page) => page.getByRole('button', { name: 'Open the browser' });

		const waitInBrowser = async (page: Page) => {
			rmSync(UNFINISHED_URL_FILE, { force: true });
			await openBrowser(page).click();
			await expect(page.getByText('Finish signing in in your browser.')).toBeVisible();
			await expect.poll(signInUrl, { timeout: 20_000, intervals: [250] }).toContain('/api/auth/oauth2/authorize?');
			return signInUrl();
		};

		const expectListenerClosed = (url: string) => expect(fetch(url)).rejects.toThrow();

		const expectUnavailable = async (page: Page) => {
			await openBrowser(page).click();
			await expect(page.getByText('Sign-in in the browser isn’t available. Paste an API key.')).toBeVisible({
				timeout: 20_000,
			});
			await expect(page.getByRole('tab', { name: 'API key' })).toHaveAttribute('aria-selected', 'true');
			await expect(page.getByRole('textbox', { name: 'API key' })).toBeVisible();
		};

		test.beforeAll(async () => {
			mkdirSync(UNFINISHED_WATCHED, { recursive: true });
			running = await launch({
				LOCALAPPDATA: join(UNFINISHED, 'appdata'),
				MIVIA_E2E_PICK_FOLDER: UNFINISHED_WATCHED,
				MIVIA_E2E_SIGN_IN_URL_FILE: UNFINISHED_URL_FILE,
				COMPUTERNAME: COMPUTER,
			});
		});

		test.afterAll(async () => {
			await shutdown(running);
			await rm(RUN, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
		});

		test('scenario 11: a server without sign-in shows the message and the API key tab', async () => {
			const { page } = running;
			await expect(page.getByRole('heading', { name: 'Welcome to MiViA Desktop' })).toBeVisible({ timeout: 30_000 });
			await page.getByRole('button', { name: 'Start setup', exact: true }).click();
			await answerClientLookup('missing');
			await expectUnavailable(page);

			await page.getByRole('tab', { name: 'Browser' }).click();
			await expect(page.getByText('Sign-in in the browser isn’t available. Paste an API key.')).toBeHidden();
			await answerClientLookup('html');
			await expectUnavailable(page);
			await expect(page.getByRole('button', { name: 'Next', exact: true })).toBeDisabled();
			expect(existsSync(UNFINISHED_URL_FILE)).toBe(false);
		});

		test('scenario 17: offline shows the notice with the details for IT on both sign-in tabs', async () => {
			const { page } = running;
			const expectOffline = async () => {
				await expect(page.getByText('MiViA cannot be reached.', { exact: false })).toBeVisible({ timeout: 20_000 });
				await expect(page.getByRole('button', { name: 'Copy details for IT' })).toBeVisible();
				await expect(page.getByText('MiViA tries again every 30 seconds.', { exact: false })).toBeHidden();
			};
			await answerClientLookup('proxyauth');
			await page.getByRole('tab', { name: 'Browser' }).click();
			await openBrowser(page).click();
			await expectOffline();
			await page.getByRole('tab', { name: 'Code' }).click();
			await expectOffline();
			await page.getByRole('tab', { name: 'Browser' }).click();
			await expect(page.getByRole('button', { name: 'Copy details for IT' })).toBeHidden();
			await answerClientLookup('normal');
		});

		test('scenario 12: cancel and a tab change close the listener', async () => {
			const { page } = running;
			await answerClientLookup('normal');
			await page.getByRole('tab', { name: 'Browser' }).click();

			const cancelled = await waitInBrowser(page);
			await page.getByRole('button', { name: 'Cancel' }).click();
			await expect(openBrowser(page)).toBeVisible();
			await expectListenerClosed(cancelled);

			const left = await waitInBrowser(page);
			await page.getByRole('tab', { name: 'API key' }).click();
			await page.getByRole('tab', { name: 'Browser' }).click();
			await expect(openBrowser(page)).toBeVisible();
			await expectListenerClosed(left);
		});

		test('scenario 18: cancelling while the key is issued does not connect', async () => {
			const { page, origin } = running;
			await fetch(`${origin}/__stub/fault`, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ op: 'signIn', kind: 'slowIssue' }),
			});
			await fetch(await waitInBrowser(page));
			await expect
				.poll(async () => (await fetch(`${origin}/__stub/issue-waiting`)).json(), { timeout: 20_000, intervals: [100] })
				.toBe(true);
			await page.getByRole('button', { name: 'Cancel' }).click();
			await expect(openBrowser(page)).toBeVisible();
			await new Promise((done) => setTimeout(done, 7000));
			await expect(page.getByText('Connected as Stub Lab')).toBeHidden();
			await expect(openBrowser(page)).toBeVisible();
		});

		test('scenario 13: a sign-in that did not finish is recorded without anything identifying', async () => {
			const { page, origin } = running;
			const first = await waitInBrowser(page);
			expect(new URL(first).searchParams.get('prompt')).toBeNull();
			await fetch(first);
			await expect(page.getByText('Connected as Stub Lab')).toBeVisible({ timeout: 20_000 });
			const move = async (button: string, heading: string) => {
				await page.getByRole('button', { name: button, exact: true }).click();
				await expect(page.getByRole('heading', { name: heading })).toBeVisible();
			};
			await move('Next', 'Where does your microscope save images?');
			await page.getByRole('button', { name: 'Choose folder…' }).click();
			await expect(page.getByText(UNFINISHED_WATCHED)).toBeVisible();
			await move('Next', 'How do images arrive in this folder?');
			await move('Next', 'Which models should analyze the images?');
			await expect(page.getByRole('checkbox', { name: 'Grain size' })).toBeChecked({ timeout: 20_000 });
			await move('Next', 'Windows and privacy');
			const consent = page.getByRole('checkbox', { name: /Help improve MiViA/ });
			await consent.click();
			await expect(consent).toBeChecked();
			await move('Back', 'Which models should analyze the images?');
			await move('Back', 'How do images arrive in this folder?');
			await move('Back', 'Where does your microscope save images?');
			await move('Back', 'Connect your MiViA account');
			await page.getByRole('button', { name: 'Use a different account' }).click();
			const stopped = await waitInBrowser(page);
			expect(new URL(stopped).searchParams.get('prompt')).toBe('login');
			await page.getByRole('button', { name: 'Cancel' }).click();
			await expect(openBrowser(page)).toBeVisible();

			const refused = new URL(await waitInBrowser(page));
			expect(refused.searchParams.get('prompt')).toBe('login');
			const denied = new URL(refused.searchParams.get('redirect_uri') ?? '');
			denied.searchParams.set('error', 'access_denied');
			denied.searchParams.set('state', refused.searchParams.get('state') ?? '');
			await fetch(denied);
			await expect(page.getByText('The sign-in was denied.')).toBeVisible({ timeout: 20_000 });

			await fetch(await waitInBrowser(page));
			await expect(page.getByText('Connected as Stub Lab')).toBeVisible({ timeout: 20_000 });
			await move('Next', 'Where does your microscope save images?');
			await move('Next', 'How do images arrive in this folder?');
			await move('Next', 'Which models should analyze the images?');
			await move('Next', 'Windows and privacy');
			await page.getByRole('button', { name: 'Finish setup', exact: true }).click();
			await expect(page.getByRole('heading', { name: 'MiViA Desktop is ready' })).toBeVisible({ timeout: 30_000 });

			type Sent = { name: string; fields: unknown };
			const failures = async () => {
				const batches: { events: Sent[] }[] = await (await fetch(`${origin}/__stub/telemetry`)).json();
				return batches
					.flatMap((batch) => batch.events)
					.filter((event) => event.name === 'sign_in.failed')
					.map((event) => event.fields);
			};
			await expect.poll(failures, { timeout: 60_000, intervals: [1000] }).toEqual([
				{ method: 'browser', reason: 'cancelled' },
				{ method: 'browser', reason: 'denied' },
			]);
			const sent = JSON.stringify(await (await fetch(`${origin}/__stub/telemetry`)).json());
			const state = new URL(stopped).searchParams.get('state') ?? 'missing';
			for (const secret of [COMPUTER, state, 'lab@example.invalid']) {
				expect(sent).not.toContain(secret);
			}
		});

		test('scenario 15: signing in again replaces the issued key, also after signing out', async () => {
			const { page, origin } = running;
			const issuedKeys = async (): Promise<{ id: string; replaces: string | null }[]> =>
				(await fetch(`${origin}/__stub/issued-keys`)).json();
			const [first, inWizard] = await issuedKeys();
			expect(first?.replaces).toBeNull();
			expect(inWizard?.replaces).toBe(first?.id);

			await page.getByRole('button', { name: 'Go to overview', exact: true }).click();
			await page
				.getByRole('link', { name: /Settings/ })
				.first()
				.click();
			await page.getByRole('link', { name: 'Account', exact: true }).click();
			await page.getByRole('button', { name: 'Sign out' }).click();
			const dialog = page.getByRole('dialog');
			await expect(
				dialog.getByText('This key keeps working for up to 90 days unless you remove it on the website.', {
					exact: false,
				}),
			).toBeVisible();
			await dialog.getByRole('button', { name: 'Sign out' }).click();
			await expect(page.getByRole('button', { name: 'Sign out' })).toHaveCount(0, { timeout: 20_000 });

			await fetch(await waitInBrowser(page));
			await expect(page.getByText('New API key saved.')).toBeVisible({ timeout: 20_000 });
			await expect(page.getByText('the old key can be deleted', { exact: false })).toBeHidden();
			expect((await issuedKeys()).at(-1)?.replaces).toBe(inWizard?.id);
			await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
		});
	});
