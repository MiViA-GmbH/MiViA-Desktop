import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { type Browser, chromium, expect, type Page, test } from '@playwright/test';

const REPO = resolve(import.meta.dirname, '../..');
const TARGET = join(REPO, 'target', 'e2e');
const RUN = join(REPO, 'target', 'e2e-run', String(Date.now()));
const WATCHED = join(RUN, 'Mikroskop');
const REPORTS = join(WATCHED, 'MiViA reports');
const CDP_PORT = 9333;
const STUB_KEY = 'stub-access-key-0123456789-abcdefghijkl';
const MODEL = '00000009-0000-4000-8000-000000000001';

type Running = { stub: ChildProcess; app: ChildProcess; origin: string; browser: Browser; page: Page };

function startStub(): Promise<{ stub: ChildProcess; origin: string }> {
	const stub = spawn(join(TARGET, 'mivia-stub.exe'), [], { env: { ...process.env, MIVIA_STUB_KEY: STUB_KEY } });
	return new Promise((done, failed) => {
		stub.stdout?.once('data', (chunk: Buffer) => done({ stub, origin: chunk.toString().trim() }));
		stub.once('error', failed);
	});
}

function startApp(origin: string, extra: Record<string, string>): ChildProcess {
	const env = { ...process.env, MIVIA_E2E_SERVER_ORIGIN: origin, MIVIA_E2E_CDP_PORT: String(CDP_PORT), ...extra };
	return spawn(join(TARGET, 'mivia-desktop.exe'), [], { env });
}

function seeded(): Record<string, string> {
	return {
		LOCALAPPDATA: join(RUN, 'appdata'),
		MIVIA_E2E_ACCESS_KEY: STUB_KEY,
		MIVIA_E2E_ACCOUNT_ID: 'stub-account',
		MIVIA_E2E_FOLDER: WATCHED,
		MIVIA_E2E_MODEL: MODEL,
		MIVIA_E2E_QUIET_SECS: '10',
	};
}

async function launch(extra: Record<string, string>): Promise<Running> {
	const { stub, origin } = await startStub();
	const app = startApp(origin, extra);
	const browser = await connect();
	const page = browser.contexts()[0]?.pages()[0];
	if (!page) {
		throw new Error('no page in the app window');
	}
	return { stub, app, origin, browser, page };
}

async function shutdown(running: Running | undefined): Promise<void> {
	await running?.browser.close().catch(() => undefined);
	await Promise.all([stop(running?.app), stop(running?.stub)]);
}

async function connect(): Promise<Browser> {
	let browser: Browser | undefined;
	await expect
		.poll(
			async () => {
				browser = await chromium.connectOverCDP(`http://127.0.0.1:${CDP_PORT}`).catch(() => undefined);
				return browser !== undefined;
			},
			{ timeout: 60_000, intervals: [500] },
		)
		.toBe(true);
	if (!browser) {
		throw new Error('the app window did not open a debugging endpoint');
	}
	return browser;
}

function stop(child: ChildProcess | undefined): Promise<void> {
	if (!child || child.exitCode !== null) {
		return Promise.resolve();
	}
	const exited = new Promise<void>((done) => child.once('exit', () => done()));
	spawnSync('taskkill', ['/pid', String(child.pid), '/t', '/f']);
	return exited;
}

function jpeg(seed: number): Buffer {
	return Buffer.from([0xff, 0xd8, 0xff, 0xe0, seed, 0x00, 0x10, seed, 0xff, 0xd9]);
}

function reports(): string[] {
	try {
		return readdirSync(REPORTS).filter((name) => name.endsWith('.pdf'));
	} catch {
		return [];
	}
}

const FRESH = join(RUN, 'fresh');
const FRESH_WATCHED = join(FRESH, 'Labor');
const FRESH_REPORTS = join(FRESH_WATCHED, 'MiViA reports');

function freshReports(): string[] {
	try {
		return readdirSync(FRESH_REPORTS).filter((name) => name.endsWith('.pdf'));
	} catch {
		return [];
	}
}

async function next(page: Page, name = 'Next'): Promise<void> {
	await page.getByRole('button', { name, exact: true }).click();
}

test.describe
	.serial('first start against the stub', () => {
		let running: Running;

		test.beforeAll(async () => {
			mkdirSync(FRESH_WATCHED, { recursive: true });
			running = await launch({ LOCALAPPDATA: join(FRESH, 'appdata'), MIVIA_E2E_PICK_FOLDER: FRESH_WATCHED });
		});

		test.afterAll(async () => {
			await shutdown(running);
		});

		test('scenario 1: the setup connects the key and the folder then delivers a report', async () => {
			const { page } = running;
			await expect(page.getByRole('heading', { name: 'Welcome to MiViA Desktop' })).toBeVisible({ timeout: 30_000 });
			await next(page, 'Start setup');
			await page.getByLabel('Access key').fill(STUB_KEY);
			await next(page);
			await expect(page.getByText('Connected as Stub Lab')).toBeVisible({ timeout: 20_000 });
			await next(page);
			await page.getByRole('button', { name: 'Choose folder…' }).click();
			await expect(page.getByText(FRESH_WATCHED)).toBeVisible();
			await next(page);
			await expect(page.getByRole('heading', { name: 'How do images arrive in this folder?' })).toBeVisible();
			await next(page);
			await expect(page.getByRole('checkbox', { name: 'Grain size' })).toBeChecked({ timeout: 20_000 });
			await next(page);
			await next(page, 'Finish setup');
			await expect(page.getByRole('heading', { name: 'MiViA Desktop is ready' })).toBeVisible({ timeout: 30_000 });
			await next(page, 'Go to overview');
			await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible();
			writeFileSync(join(FRESH_WATCHED, 'probe_001.jpg'), jpeg(7));
			await expect(page.getByText(/: 1 of 1 done/)).toBeVisible({ timeout: 60_000 });
			await page.getByRole('button', { name: 'Finish series now' }).first().click();
			await expect.poll(freshReports, { timeout: 120_000, intervals: [1000] }).toHaveLength(1);
		});
	});

test.describe
	.serial('desktop smoke against the stub', () => {
		let running: Running;

		test.beforeAll(async () => {
			mkdirSync(WATCHED, { recursive: true });
			running = await launch(seeded());
		});

		test.afterAll(async () => {
			await shutdown(running);
			rmSync(RUN, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
		});

		test('scenario 2: a watched folder series ends with a saved PDF report', async () => {
			const { page, origin } = running;
			await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible({ timeout: 30_000 });
			await expect(page.getByText('Mikroskop').first()).toBeVisible();
			for (const seed of [1, 2, 3]) {
				writeFileSync(join(WATCHED, `probe_00${seed}.jpg`), jpeg(seed));
			}
			await expect(page.getByText(/: \d of 3 done/)).toBeVisible({ timeout: 60_000 });
			await page.getByRole('button', { name: 'Finish series now' }).first().click();
			await expect.poll(reports, { timeout: 120_000, intervals: [1000] }).toHaveLength(1);
			const [name] = reports();
			expect(name).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}-\d{2} .+ - Grain size\.pdf$/);
			const saved = readFileSync(join(REPORTS, name ?? '')).toString('latin1');
			const served: string[] = await (await fetch(`${origin}/__stub/served`)).json();
			expect(served).toContain(saved);
		});

		test('scenario 5: every query command answers through real IPC', async () => {
			const { page } = running;
			const outcomes = await page.evaluate(async () => {
				type Internals = { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> };
				const { invoke } = (window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__;
				const history = (await invoke('list_history', { after: null })) as { rows: { id: string }[] };
				const id = history.rows[0]?.id ?? '0';
				const calls: [string, Record<string, unknown>][] = [
					['get_locale', {}],
					['get_account', {}],
					['list_folders', {}],
					['list_holds', {}],
					['list_needs_attention', {}],
					['list_history', { after: null }],
					['get_batch', { id }],
					['list_batch_items', { id, after: null }],
					['list_series_reports', { id }],
				];
				const results: Record<string, string> = {};
				for (const [command, args] of calls) {
					results[command] = await invoke(command, args).then(
						() => 'ok',
						(error: unknown) => JSON.stringify(error),
					);
				}
				return results;
			});
			expect(Object.values(outcomes)).toEqual(Array(9).fill('ok'));
		});
	});
