import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { crc32, deflateSync } from 'node:zlib';
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
	const page = await firstPage(browser);
	return { stub, app, origin, browser, page };
}

async function firstPage(browser: Browser): Promise<Page> {
	let page: Page | undefined;
	await expect
		.poll(
			() => {
				page = browser.contexts()[0]?.pages()[0];
				return page !== undefined;
			},
			{ timeout: 30_000, intervals: [250] },
		)
		.toBe(true);
	if (!page) {
		throw new Error('no page in the app window');
	}
	return page;
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

function png(width: number, height: number): Buffer {
	const raw = Buffer.alloc(height * (1 + width * 3));
	for (let y = 0; y < height; y += 1) {
		for (let x = 0; x < width; x += 1) {
			raw.set([Math.floor((x * 255) / width), Math.floor((y * 255) / height), 96], y * (1 + width * 3) + 1 + x * 3);
		}
	}
	const chunk = (type: string, body: Buffer) => {
		const named = Buffer.concat([Buffer.from(type), body]);
		const length = Buffer.alloc(4);
		length.writeUInt32BE(body.length);
		const check = Buffer.alloc(4);
		check.writeUInt32BE(crc32(named));
		return Buffer.concat([length, named, check]);
	};
	const header = Buffer.alloc(13);
	header.writeUInt32BE(width, 0);
	header.writeUInt32BE(height, 4);
	header.set([8, 2, 0, 0, 0], 8);
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', header),
		chunk('IDAT', deflateSync(raw)),
		chunk('IEND', Buffer.alloc(0)),
	]);
}

async function publishResults(origin: string): Promise<number> {
	const listed = await fetch(`${origin}/api/v2/jobs?modelId=${MODEL}&page=1`, { headers: { authorization: STUB_KEY } });
	const jobs: { id: string }[] = ((await listed.json()) as { data: { id: string }[] }).data;
	const results = [
		{ type: 'data-table', data: [{ label: 'GRAIN_MEAN', unit: 'um', value: 12.5 }] },
		{ type: 'image', label: 'GRAIN_OVERLAY', filename: 'overlay.png' },
		{ type: 'message', label: '' },
	];
	const picture = png(3000, 2000);
	for (const { id } of jobs) {
		const body = JSON.stringify({ job_id: id, results });
		await fetch(`${origin}/__stub/result`, { method: 'POST', headers: { 'content-type': 'application/json' }, body });
		await fetch(`${origin}/__stub/result-image?job_id=${id}&name=overlay.png`, {
			method: 'POST',
			body: new Uint8Array(picture),
		});
	}
	return jobs.length;
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
			const consent = page.getByRole('checkbox', { name: /send technical and usage data/ });
			await consent.click();
			await expect(consent).toBeChecked();
			await next(page, 'Finish setup');
			await expect(page.getByRole('heading', { name: 'MiViA Desktop is ready' })).toBeVisible({ timeout: 30_000 });
			await next(page, 'Go to overview');
			await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible();
			writeFileSync(join(FRESH_WATCHED, 'probe_001.jpg'), jpeg(7));
			await expect(page.getByText(/: 1 of 1 done/)).toBeVisible({ timeout: 60_000 });
			await page.getByRole('button', { name: 'Create report now' }).first().click();
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
				.toEqual(expect.arrayContaining(['wizard.finished', 'account.connected', 'folder.added']));
			const text = JSON.stringify(await received());
			expect(text).not.toContain('session.start');
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
			await page.getByRole('button', { name: 'Create report now' }).first().click();
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
					['get_update', {}],
				];
				const folders = (await invoke('list_folders')) as { folders: { id: string }[] };
				const folderId = folders.folders[0]?.id ?? '0';
				calls.push(['check_folder_again', { id: folderId }], ['dismiss_watched_elsewhere', { id: folderId }]);
				const results: Record<string, string> = {};
				for (const [command, args] of calls) {
					results[command] = await invoke(command, args).then(
						() => 'ok',
						(error: { code?: string }) => {
							if (error.code === 'updatesNotReady') {
								return 'ok';
							}
							return JSON.stringify(error);
						},
					);
				}
				return results;
			});
			expect(Object.values(outcomes)).toEqual(Array(24).fill('ok'));
		});

		test('scenario 8: a result image is previewed through the asset protocol and the source falls back', async () => {
			const { page, origin } = running;
			expect(await publishResults(origin)).toBeGreaterThan(0);
			await page.getByRole('link', { name: 'History', exact: true }).click();
			await page.locator('a[href^="/series/"]').first().click();
			await page
				.getByRole('link', { name: /^Preview / })
				.first()
				.click();
			await expect(page.getByText('Preview not available')).toBeVisible({ timeout: 30_000 });
			await expect(page.getByText('GRAIN_MEAN')).toBeVisible();
			await expect(page.getByText('This result cannot be shown')).toBeVisible();
			await page.getByRole('button', { name: /GRAIN_OVERLAY/ }).click();
			const picture = page.locator('img[src*="asset.localhost"], img[src^="asset:"]').first();
			await expect(picture).toBeVisible({ timeout: 30_000 });
			await expect.poll(() => picture.evaluate((image: HTMLImageElement) => image.naturalWidth)).toBe(2048);
			const zoomIn = page.getByRole('button', { name: 'Zoom in' });
			for (let step = 0; step < 10 && (await zoomIn.isEnabled()); step += 1) {
				await zoomIn.click();
			}
			await expect(zoomIn).toBeDisabled();
			await expect.poll(() => picture.evaluate((image: HTMLImageElement) => image.naturalWidth)).toBe(3000);
		});

		test('scenario 6: a problem report is sent, saved after a 404 and the local logs are deleted', async () => {
			const { page, origin } = running;
			await page
				.getByRole('link', { name: /Settings/ })
				.first()
				.click();
			await page.getByRole('link', { name: 'Support' }).click();
			await page.getByRole('button', { name: 'Administrator policies' }).click();
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
