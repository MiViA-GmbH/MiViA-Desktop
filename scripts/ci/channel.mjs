import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';

const SOAK_HOURS = 48;

const fail = (message) => {
	process.stderr.write(`channel: ${message}\n`);
	process.exit(1);
};

const channel = process.argv[2];
const tag = process.env.TAG ?? '';
const repo = process.env.REPO ?? '';
if (!['stable', 'beta'].includes(channel)) {
	fail('usage: node scripts/ci/channel.mjs <stable|beta>');
}
if (!/^(beta-)?v\d+\.\d+\.\d+(-beta\.\d+)?$/.test(tag)) {
	fail(`TAG ${JSON.stringify(tag)} is not a release tag`);
}
const version = tag.replace(/^beta-/, '').replace(/^v/, '');
if (channel === 'stable' && version.includes('-')) {
	fail(`${tag} is a prerelease and cannot go to stable`);
}

if (channel === 'stable' && process.env.SKIP_SOAK !== 'true') {
	const added = execFileSync(
		'git',
		['log', '--reverse', '--format=%ct', '-S', `"version": "${version}"`, '--', 'channels/beta.json'],
		{ encoding: 'utf8' },
	)
		.split('\n')
		.find(Boolean);
	if (added === undefined) {
		fail(`${version} was never on the beta channel`);
	}
	const hours = (Date.now() / 1000 - Number(added)) / 3600;
	if (hours < SOAK_HOURS) {
		fail(`${version} has been on beta for ${hours.toFixed(1)} h, the soak is ${SOAK_HOURS} h`);
	}
}

const headers = { Accept: 'application/vnd.github+json' };
if (process.env.GH_TOKEN) {
	headers.Authorization = `Bearer ${process.env.GH_TOKEN}`;
}
const response = await fetch(`https://api.github.com/repos/${repo}/releases/tags/${tag}`, { headers });
if (!response.ok) {
	fail(`release ${tag}: HTTP ${response.status}`);
}
const release = await response.json();
if (release.draft) {
	fail(`${tag} is still a draft`);
}
const installers = release.assets.filter((asset) => asset.name.endsWith('-setup.exe'));
if (installers.length !== 1) {
	fail(`${tag} has ${installers.length} installers`);
}
const [installer] = installers;
const sig = release.assets.find((asset) => asset.name === `${installer.name}.sig`);
if (sig === undefined) {
	fail(`${tag} has no ${installer.name}.sig`);
}
const signature = await fetch(sig.browser_download_url);
if (!signature.ok) {
	fail(`${sig.name}: HTTP ${signature.status}`);
}

const entry = {
	version,
	notes: release.name ?? tag,
	pub_date: release.published_at,
	url: `https://github.com/${repo}/releases/download/${tag}/${installer.name}`,
	signature: (await signature.text()).trim(),
};
mkdirSync('channels', { recursive: true });
writeFileSync(`channels/${channel}.json`, `${JSON.stringify(entry, null, '\t')}\n`);
process.stdout.write(`channel: ${channel} -> ${tag}\n`);
