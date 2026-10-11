import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { crc32, deflateSync } from 'node:zlib';
import { chromium } from '@playwright/test';

const EDGE = 8192;
const SETTLE_MS = 1500;
const DECODE_TIMEOUT_MS = 60_000;

function chunk(type, body) {
	const named = Buffer.concat([Buffer.from(type), body]);
	const length = Buffer.alloc(4);
	length.writeUInt32BE(body.length);
	const check = Buffer.alloc(4);
	check.writeUInt32BE(crc32(named));
	return Buffer.concat([length, named, check]);
}

function picture(seed) {
	const stride = 1 + EDGE * 3;
	const raw = Buffer.alloc(EDGE * stride);
	for (let y = 0; y < EDGE; y += 1) {
		const row = y * stride;
		const band = Math.floor(y / 64) % 2 === 0 ? 40 : 0;
		for (let x = 0; x < EDGE; x += 1) {
			const grain = Math.floor(x / 48 + y / 48 + seed) % 7 === 0 ? 60 : 0;
			raw.set([(x >> 5) & 255, (y >> 5) & 255, (seed * 37 + band + grain) & 255], row + 1 + x * 3);
		}
	}
	const header = Buffer.alloc(13);
	header.writeUInt32BE(EDGE, 0);
	header.writeUInt32BE(EDGE, 4);
	header.set([8, 2, 0, 0, 0], 8);
	const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
	return Buffer.concat([
		signature,
		chunk('IHDR', header),
		chunk('IDAT', deflateSync(raw, { level: 1 })),
		chunk('IEND', Buffer.alloc(0)),
	]);
}

function seed(folder, count) {
	mkdirSync(folder, { recursive: true });
	for (let index = 0; index < count; index += 1) {
		writeFileSync(join(folder, `perf_${String(index).padStart(3, '0')}.png`), picture(index));
	}
}

const answers = createInterface({ input: process.stdin })[Symbol.asyncIterator]();

async function sample(tag) {
	process.stdout.write(`SAMPLE ${tag}\n`);
	await answers.next();
}

function say(tag, value) {
	process.stdout.write(`${tag} ${value}\n`);
}

async function connect(port) {
	const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
	const page = browser.contexts()[0]?.pages()[0];
	if (!page) {
		throw new Error('no app window');
	}
	return { browser, page };
}

function invoke(page, command, args) {
	return page.evaluate(([name, input]) => window.__TAURI_INTERNALS__.invoke(name, input), [command, args]);
}

async function seriesOf(page, count) {
	const deadline = Date.now() + 5 * 60_000;
	while (Date.now() < deadline) {
		const { folders } = await invoke(page, 'list_folders', {});
		const id = folders[0]?.openSeries?.id ?? folders[0]?.lastSeries?.id;
		const items = id ? await invoke(page, 'list_batch_items', { id, after: null }) : { items: [] };
		if (items.items.length >= count) {
			return id;
		}
		await page.waitForTimeout(1000);
	}
	throw new Error('the series did not reach the wanted size');
}

async function open(page, index) {
	const link = page.getByRole('link', { name: /^Preview / }).nth(index);
	const started = performance.now();
	await link.click();
	await page.waitForFunction(
		() =>
			[...document.querySelectorAll('img')].some(
				(image) => image.src.includes('asset.localhost') && image.complete && image.naturalWidth > 0,
			),
		undefined,
		{ polling: 'raf', timeout: DECODE_TIMEOUT_MS },
	);
	return performance.now() - started;
}

async function back(page) {
	await page.getByRole('link', { name: 'Back to series' }).click();
	await page
		.getByRole('link', { name: /^Preview / })
		.first()
		.waitFor();
}

async function first(page, index, measured) {
	if (measured) {
		await sample('before');
	}
	say('uncached', await open(page, index));
	await page.waitForTimeout(SETTLE_MS);
	if (measured) {
		await sample('after');
	}
	await sample('soak');
	await back(page);
}

async function run(port, count, measured) {
	const { browser, page } = await connect(port);
	const id = await seriesOf(page, count);
	await page.goto(new URL(`/series/${id}`, page.url()).href);
	await page
		.getByRole('link', { name: /^Preview / })
		.first()
		.waitFor();
	await sample('baseline');
	for (let index = 0; index < count; index += 1) {
		await first(page, index, index < measured);
	}
	for (let index = 0; index < measured; index += 1) {
		say('cached', await open(page, index));
		await back(page);
	}
	await browser.close();
}

const [command, ...args] = process.argv.slice(2);
if (command === 'seed') {
	seed(args[0], Number(args[1]));
} else if (command === 'run') {
	await run(Number(args[0]), Number(args[1]), Number(args[2]));
	process.exit(0);
} else {
	process.stderr.write('usage: perf-preview.mjs seed <folder> <count> | run <port> <count> <measured>\n');
	process.exit(2);
}
