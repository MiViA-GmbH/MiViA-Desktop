import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PREFIX = 'https://github.com/MiViA-GmbH/MiViA-Desktop/releases/download/';
const VERSION = /^\d+\.\d+\.\d+(-beta\.\d+)?$/;

const fail = (message) => {
	process.stderr.write(`render-channels: ${message}\n`);
	process.exit(1);
};

const site = process.argv[2];
if (site === undefined) {
	fail('usage: node scripts/ci/render-channels.mjs <site dir>');
}

for (const channel of ['stable', 'beta']) {
	const source = join('channels', `${channel}.json`);
	if (!existsSync(source)) {
		continue;
	}
	const entry = JSON.parse(readFileSync(source, 'utf8'));
	const version = VERSION.test(entry.version) && !(channel === 'stable' && entry.version.includes('-'));
	const url = typeof entry.url === 'string' && entry.url.startsWith(PREFIX);
	const signed = typeof entry.signature === 'string' && entry.signature.length > 0;
	const dated = typeof entry.pub_date === 'string';
	const located = version && url;
	if (!(located && signed && dated)) {
		fail(`${source} is not a valid channel entry`);
	}
	const target = { signature: entry.signature, url: entry.url };
	const manifest = {
		version: entry.version,
		notes: entry.notes ?? '',
		pub_date: entry.pub_date,
		platforms: { 'windows-x86_64': target, 'windows-x86_64-nsis': target },
	};
	mkdirSync(join(site, channel), { recursive: true });
	writeFileSync(join(site, channel, 'latest.json'), `${JSON.stringify(manifest)}\n`);
	process.stdout.write(`render-channels: ${channel} ${entry.version}\n`);
}
