import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { type Browser, chromium, expect, type Page, test } from '@playwright/test';

const REPO = resolve(import.meta.dirname, '../..');
const TARGET = join(REPO, 'target', 'e2e');
const RUN = join(REPO, 'target', 'e2e-run', String(Date.now()));
const WATCHED = join(RUN, 'Mikroskop');
const REPORTS = join(WATCHED, 'MiViA reports');
const SENT = join(RUN, 'Gesendet');
const SENT_REPORTS = join(SENT, 'MiViA reports');
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

function pdfs(folder: string): string[] {
	try {
		return readdirSync(folder).filter((name) => name.endsWith('.pdf'));
	} catch {
		return [];
	}
}

const reports = () => pdfs(REPORTS);

const FRESH = join(RUN, 'fresh');
const FRESH_WATCHED = join(FRESH, 'Labor');
const FRESH_REPORTS = join(FRESH_WATCHED, 'MiViA reports');

const freshReports = () => pdfs(FRESH_REPORTS);

function sendTo(paths: string[]): number | null {
	const env = { ...process.env, LOCALAPPDATA: join(RUN, 'appdata') };
	const args = ['--open-paths', '--', ...paths];
	return spawnSync(join(TARGET, 'mivia-desktop.exe'), args, { env, timeout: 30_000 }).status;
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

		test('scenario 7: the setup events reach the server without names, then the switch turns telemetry off', async () => {
			const { page, origin } = running;
			const received = async () => {
				const batches: { events: { name: string }[] }[] = await (await fetch(`${origin}/__stub/telemetry`)).json();
				return batches;
			};
			await expect
				.poll(async () => (await received()).flatMap((batch) => batch.events.map((event) => event.name)), {
					timeout: 60_000,
					intervals: [1000],
				})
				.toEqual(expect.arrayContaining(['wizard.finished', 'account.connected', 'folder.added', 'session.start']));
			const text = JSON.stringify(await received());
			for (const secret of [FRESH_WATCHED, 'Labor', STUB_KEY, 'probe_001']) {
				expect(text).not.toContain(secret);
			}
			await page
				.getByRole('link', { name: /Settings/ })
				.first()
				.click();
			await page.getByRole('link', { name: 'Privacy' }).click();
			const sw = page.getByRole('switch');
			await expect(sw).toBeChecked();
			await expect(page.getByText(/^[0-9a-f]{32}$/)).toBeVisible();
			await page.getByRole('button', { name: 'View sent data' }).click();
			const sent = page.getByRole('dialog', { name: 'View sent data' });
			await expect(sent.getByRole('button', { name: /events?$/ }).first()).toBeVisible();
			await page.keyboard.press('Escape');
			await sw.click({ force: true });
			await expect(sw).not.toBeChecked();
			await expect(page.getByText('nothing is recorded or sent', { exact: false })).toBeVisible();
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
			const markers = readdirSync(REPORTS).filter((file) => /^\.mivia-watch-[0-9a-f]{12}\.json$/.test(file));
			expect(markers).toHaveLength(1);
			const marker = JSON.parse(readFileSync(join(REPORTS, markers[0] ?? ''), 'utf8'));
			expect(marker.watchedPathUnc.toLowerCase()).toBe(WATCHED.toLowerCase());
		});

		test('scenario 3: images sent from a second process end in a report beside them', async () => {
			const { page } = running;
			mkdirSync(SENT, { recursive: true });
			const sent = [join(SENT, 'probe_002.jpg'), join(SENT, 'probe_001.jpg')];
			for (const [index, path] of sent.entries()) {
				writeFileSync(path, jpeg(20 + index));
			}
			expect(sendTo(sent)).toBe(0);
			const dialog = page.getByRole('dialog', { name: 'Analyze 2 images' });
			await expect(dialog).toBeVisible({ timeout: 30_000 });
			await expect(dialog.getByText('probe_001.jpg')).toBeVisible();
			await expect(dialog.getByRole('checkbox', { name: 'Grain size' })).toBeChecked({ timeout: 20_000 });
			await dialog.getByRole('button', { name: 'Analyze 2 images' }).click();
			await expect(dialog).toBeHidden();
			await expect.poll(() => pdfs(SENT_REPORTS), { timeout: 120_000, intervals: [1000] }).toHaveLength(1);
			expect(pdfs(SENT_REPORTS)[0]).toMatch(/ probe_001 \+1 - Grain size\.pdf$/);
		});

		test('scenario 5: every query command answers through real IPC', async () => {
			const { page } = running;
			const outcomes = await page.evaluate(async () => {
				type Internals = { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> };
				const { invoke } = (window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__;
				const filter = {
					q: null,
					folder: null,
					manual: false,
					analysis: null,
					status: null,
					from: null,
					to: null,
					sort: 'finished',
					ascending: false,
				};
				const history = (await invoke('list_history', { filter, after: null })) as {
					rows: { summary: { id: string } }[];
				};
				const id = history.rows[0]?.summary.id ?? '0';
				const calls: [string, Record<string, unknown>][] = [
					['get_locale', {}],
					['get_account', {}],
					['list_folders', {}],
					['list_holds', {}],
					['list_needs_attention', {}],
					['list_history', { filter: { ...filter, q: 'probe', sort: 'name' }, after: null }],
					['list_custom_reports', { q: null, after: null }],
					['list_batch_events', { id, after: null }],
					['get_custom_report_draft', { series: [id] }],
					['list_backups', {}],
					['get_batch', { id }],
					['list_batch_items', { id, after: null }],
					['list_series_reports', { id }],
					['get_one_off', {}],
					['get_remaining_uploads', {}],
					['get_support', {}],
					['list_pending_reports', {}],
					['get_privacy', {}],
					['list_sent_telemetry', {}],
					['get_network', {}],
					['get_network_details', {}],
				];
				const folders = (await invoke('list_folders')) as { folders: { id: string }[] };
				const folderId = folders.folders[0]?.id ?? '0';
				calls.push(['check_folder_again', { id: folderId }], ['dismiss_watched_elsewhere', { id: folderId }]);
				const results: Record<string, string> = {};
				for (const [command, args] of calls) {
					results[command] = await invoke(command, args).then(
						() => 'ok',
						(error: unknown) => JSON.stringify(error),
					);
				}
				return results;
			});
			expect(Object.values(outcomes)).toEqual(Array(23).fill('ok'));
		});

		test('scenario 6: a problem report is sent, saved after a 404 and the local logs are deleted', async () => {
			const { page, origin } = running;
			await page
				.getByRole('link', { name: /Settings/ })
				.first()
				.click();
			await page.getByRole('link', { name: 'Support' }).click();
			await expect(page.getByRole('row')).toHaveCount(15);
			const report = async (description: string) => {
				await page.getByRole('button', { name: 'Report a problem…' }).click();
				const dialog = page.getByRole('dialog', { name: 'Report a problem' });
				await dialog.getByLabel('What happened?').fill(description);
				await dialog.getByRole('button', { name: 'Review what will be sent' }).click();
				await expect(dialog.getByText(/^\d+ files, /)).toBeVisible({ timeout: 30_000 });
				await dialog.getByRole('button', { name: 'Send', exact: true }).click();
				await expect(dialog).toBeHidden();
			};
			await report('Report stopped after the upload.');
			await expect(page.getByText(/^Problem report sent\. Reference [0-9a-f]{8}\.$/)).toBeVisible({ timeout: 30_000 });
			const journal: { op: string }[] = await (await fetch(`${origin}/__stub/journal`)).json();
			expect(journal.some((entry) => entry.op === 'ProblemReport')).toBe(true);
			const fault = { op: 'problemReport', kind: 'before', status: 404 };
			await fetch(`${origin}/__stub/fault`, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(fault),
			});
			await report('Second report.');
			await page.getByRole('button', { name: 'Save and e-mail' }).first().click();
			await expect(page.getByText('The report is in your Downloads folder.', { exact: false })).toBeVisible();
			expect(
				readdirSync(join(RUN, 'appdata'), { recursive: true })
					.map(String)
					.some((name) => /downloads[\\/]MiViA-report-.+\.zip$/.test(name)),
			).toBe(true);
			await page.getByRole('button', { name: 'Delete local logs' }).click();
			await page
				.getByRole('dialog', { name: 'Delete local logs?' })
				.getByRole('button', { name: 'Delete logs' })
				.click();
			await expect(page.getByText(/files? deleted\.$/)).toBeVisible();
		});
	});
