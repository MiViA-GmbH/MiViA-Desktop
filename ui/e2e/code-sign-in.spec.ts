import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { launch, next, RUN, type Running, shutdown } from './desktop-app';

const COMPUTER = 'LAB-PC-8';
const CODE_RUN = join(RUN, 'code');
const CODE_URL_FILE = join(CODE_RUN, 'opened-url.txt');

function openedUrl(): string {
	try {
		return readFileSync(CODE_URL_FILE, 'utf8');
	} catch {
		return '';
	}
}

test.describe
	.serial('code sign-in against the stub', () => {
		let running: Running;

		const fault = (kind: string) =>
			fetch(`${running.origin}/__stub/fault`, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ op: 'deviceToken', kind }),
			});
		const polls = async (): Promise<number> => (await fetch(`${running.origin}/__stub/device/polls`)).json();

		test.beforeAll(async () => {
			mkdirSync(join(CODE_RUN, 'Labor'), { recursive: true });
			running = await launch({
				LOCALAPPDATA: join(CODE_RUN, 'appdata'),
				MIVIA_E2E_PICK_FOLDER: join(CODE_RUN, 'Labor'),
				MIVIA_E2E_SIGN_IN_URL_FILE: CODE_URL_FILE,
				COMPUTERNAME: COMPUTER,
			});
		});

		test.afterAll(async () => {
			await shutdown(running);
		});

		test('scenario 10: typing the code on the device page connects the wizard', async () => {
			const { page, origin } = running;
			await expect(page.getByRole('heading', { name: 'Welcome to MiViA Desktop' })).toBeVisible({ timeout: 30_000 });
			await next(page, 'Start setup');
			await page.getByRole('tab', { name: 'Code' }).click();
			await expect(page.getByText('WDJB-MJHT')).toBeVisible({ timeout: 20_000 });
			await expect(page.getByRole('button', { name: 'Copy code' })).toBeVisible();
			await expect(page.getByText('Waiting for approval.')).toBeVisible();

			const host = new URL(origin).host;
			await page.getByRole('button', { name: `${host}/device` }).click();
			await expect.poll(openedUrl, { timeout: 20_000, intervals: [250] }).toBe(`${origin}/device`);

			await fault('access_denied');
			await expect(page.getByText('The sign-in was denied.')).toBeVisible({ timeout: 20_000 });
			await page.getByRole('button', { name: 'Try again' }).click();
			await expect(page.getByText('Waiting for approval.')).toBeVisible({ timeout: 20_000 });

			await fault('expired_token');
			await expect(page.getByText('The code has expired.')).toBeVisible({ timeout: 20_000 });
			await page.getByRole('button', { name: 'Try again' }).click();
			await expect(page.getByText('Waiting for approval.')).toBeVisible({ timeout: 20_000 });

			await page.getByRole('tab', { name: 'API key' }).click();
			const seen = await polls();
			await new Promise((done) => setTimeout(done, 3000));
			expect(await polls()).toBeLessThanOrEqual(seen + 1);
			const settled = await polls();
			await new Promise((done) => setTimeout(done, 3000));
			expect(await polls()).toBe(settled);

			await page.getByRole('tab', { name: 'Code' }).click();
			await expect(page.getByText('Waiting for approval.')).toBeVisible({ timeout: 20_000 });
			await fault('slow_down');
			await expect.poll(polls, { timeout: 20_000, intervals: [250] }).toBeGreaterThan(settled);
			await fetch(`${origin}/__stub/device/approve`, { method: 'POST' });
			await expect(page.getByText('Connected as Stub Lab')).toBeVisible({ timeout: 30_000 });
			expect(await (await fetch(`${origin}/__stub/issued`)).json()).toEqual([`MiViA Desktop on ${COMPUTER}`]);

			await next(page);
			await page.getByRole('button', { name: 'Choose folder…' }).click();
			await next(page);
			await expect(page.getByRole('heading', { name: 'How do images arrive in this folder?' })).toBeVisible();
			await next(page);
			await expect(page.getByRole('checkbox', { name: 'Grain size' })).toBeChecked({ timeout: 20_000 });
			await next(page);
			const consent = page.getByRole('checkbox', { name: /Help improve MiViA/ });
			await consent.click();
			await expect(consent).toBeChecked();
			await next(page, 'Finish setup');
			await expect(page.getByRole('heading', { name: 'MiViA Desktop is ready' })).toBeVisible({ timeout: 30_000 });

			const connected = async () => {
				const batches: { events: { name: string }[] }[] = await (await fetch(`${origin}/__stub/telemetry`)).json();
				return JSON.stringify(batches.flatMap((batch) => batch.events).find((e) => e.name === 'account.connected'));
			};
			await expect.poll(connected, { timeout: 60_000, intervals: [1000] }).toContain('"code"');
		});
	});
