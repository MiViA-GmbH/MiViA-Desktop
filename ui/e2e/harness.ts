import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { type Browser, chromium, expect, type Page } from '@playwright/test';

const REPO = resolve(import.meta.dirname, '../..');
export const TARGET = join(REPO, 'target', 'e2e');
export const RUN = join(REPO, 'target', 'e2e-run', String(Date.now()));
const CDP_PORT = 9333;
export const STUB_KEY = 'stub-access-key-0123456789-abcdefghijkl';

export type Running = { stub: ChildProcess; app: ChildProcess; origin: string; browser: Browser; page: Page };

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

export async function launch(extra: Record<string, string>): Promise<Running> {
	const { stub, origin } = await startStub();
	const app = startApp(origin, extra);
	try {
		const browser = await connect();
		const page = await firstPage(browser);
		return { stub, app, origin, browser, page };
	} catch (error) {
		await Promise.all([stop(app), stop(stub)]);
		throw error;
	}
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

export async function shutdown(running: Running | undefined): Promise<void> {
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

export function stop(child: ChildProcess | undefined): Promise<void> {
	if (!child || child.exitCode !== null) {
		return Promise.resolve();
	}
	const exited = new Promise<void>((done) => child.once('exit', () => done()));
	spawnSync('taskkill', ['/pid', String(child.pid), '/t', '/f']);
	return exited;
}

export async function next(page: Page, name = 'Next'): Promise<void> {
	await page.getByRole('button', { name, exact: true }).click();
}
