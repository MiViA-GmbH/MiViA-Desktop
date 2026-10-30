import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { launch, REPO, type Running, STUB_KEY, shutdown } from './stub-app';

const RUN = join(REPO, 'target', 'e2e-run', `switch-sign-in-${Date.now()}`);
const WATCHED = join(RUN, 'Labor');
const URL_FILE = join(RUN, 'sign-in-url.txt');
const MODEL = '00000009-0000-4000-8000-000000000001';
const JPEG = Buffer.from([0xff, 0xd8, 0xff, 0xe0, 30, 0x00, 0x10, 30, 0xff, 0xd9]);

function signInUrl(): string {
	try {
		return readFileSync(URL_FILE, 'utf8');
	} catch {
		return '';
	}
}

test.describe
	.serial('signing in to a different account against the stub', () => {
		let running: Running;

		test.beforeAll(async () => {
			mkdirSync(WATCHED, { recursive: true });
			running = await launch({
				LOCALAPPDATA: join(RUN, 'appdata'),
				MIVIA_E2E_ACCESS_KEY: STUB_KEY,
				MIVIA_E2E_ACCOUNT_ID: 'stub-account',
				MIVIA_E2E_FOLDER: WATCHED,
				MIVIA_E2E_MODEL: MODEL,
				MIVIA_E2E_QUIET_SECS: '10',
				MIVIA_E2E_SIGN_IN_URL_FILE: URL_FILE,
			});
		});

		test.afterAll(async () => {
			await shutdown(running);
			await rm(RUN, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
		});

		test('scenario 15: cancelling the switch issues no key and keeps the account and the hold', async () => {
			const { page, origin } = running;
			const issued = async () => (await fetch(`${origin}/__stub/issued`)).json();
			await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible({ timeout: 30_000 });

			await fetch(`${origin}/__stub/revoke-key`, { method: 'POST' });
			writeFileSync(join(WATCHED, 'probe_001.jpg'), JPEG);
			const again = page.getByRole('button', { name: 'Sign in again' });
			await expect(again).toBeVisible({ timeout: 60_000 });
			await fetch(`${origin}/__stub/fault`, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ op: 'signIn', kind: 'differentAccount' }),
			});

			await again.click();
			await page.getByRole('button', { name: 'Use a different account' }).click();
			await expect.poll(signInUrl, { timeout: 20_000, intervals: [250] }).toContain('authorize?');
			expect(new URL(signInUrl()).searchParams.get('prompt')).toBe('login');
			expect((await fetch(signInUrl())).status).toBe(200);

			const dialog = page.getByRole('dialog', { name: 'Switch account?' });
			await expect(dialog).toContainText('You signed in as Other Lab, not Stub Lab.', { timeout: 20_000 });
			await expect(dialog).toContainText('are no longer watched');
			expect(await issued()).toEqual([]);

			await dialog.getByRole('button', { name: 'Cancel' }).first().click();
			await expect(dialog).toBeHidden();
			expect(await issued()).toEqual([]);
			await expect(page.getByText('lab@example.invalid')).toBeVisible();
			await expect(again).toBeVisible();

			rmSync(URL_FILE);
			await page.getByRole('button', { name: 'Open the browser' }).click();
			await expect.poll(signInUrl, { timeout: 20_000, intervals: [250] }).toContain('authorize?');
			expect(new URL(signInUrl()).searchParams.get('prompt')).toBeNull();
			expect((await fetch(signInUrl())).status).toBe(200);
			await expect(dialog).toBeVisible({ timeout: 20_000 });
		});
	});
