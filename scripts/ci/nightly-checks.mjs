import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

const repo = process.env.GITHUB_REPOSITORY ?? '';
const SITE = 'https://updates.mivia.ai';
const TAG = /^(beta-)?v\d+\.\d+\.\d+(-beta\.\d+)?$/;
const problems = [];

const releases = JSON.parse(
	execFileSync('gh', ['api', '--paginate', '--slurp', `repos/${repo}/releases?per_page=100`], { encoding: 'utf8' }),
).flat();
const tags = execFileSync('gh', ['api', '--paginate', `repos/${repo}/git/matching-refs/tags/`, '--jq', '.[].ref'], {
	encoding: 'utf8',
})
	.split('\n')
	.filter(Boolean)
	.map((ref) => ref.replace('refs/tags/', ''));

for (const tag of tags.filter((tag) => TAG.test(tag))) {
	if (!releases.some((release) => release.tag_name === tag)) {
		problems.push(`tag ${tag} has no release`);
	}
}
for (const release of releases.filter((release) => !release.draft)) {
	const names = release.assets.map((asset) => asset.name);
	const installers = names.filter((name) => name.endsWith('-setup.exe'));
	if (!TAG.test(release.tag_name)) {
		problems.push(`release ${release.tag_name} has no release tag name`);
	}
	if (installers.length !== 1 || !names.includes(`${installers[0]}.sig`)) {
		problems.push(`release ${release.tag_name} needs exactly one installer and its .sig`);
	}
}

const site = mkdtempSync(join(tmpdir(), 'channels-'));
const rendered = spawnSync('node', ['scripts/ci/render-channels.mjs', site], { stdio: 'inherit' });
if (rendered.status !== 0) {
	problems.push('channels/*.json on main do not render or their signatures do not verify');
}
const live = async (channel) => {
	try {
		const response = await fetch(`${SITE}/${channel}/latest.json`, { cache: 'no-store' });
		if (response.status === 404) {
			return { found: undefined };
		}
		if (!response.ok) {
			return { error: `HTTP ${response.status}` };
		}
		return { found: await response.json() };
	} catch (error) {
		return { error: error.cause?.code ?? error.message };
	}
};

for (const channel of ['stable', 'beta']) {
	const file = join(site, channel, 'latest.json');
	const expected = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : undefined;
	const served = await live(channel);
	if (served.error !== undefined) {
		problems.push(`${SITE}/${channel}/latest.json: ${served.error}`);
	} else if (!isDeepStrictEqual(served.found, expected)) {
		problems.push(`${SITE}/${channel}/latest.json differs from channels/${channel}.json on main`);
	}
}

for (const problem of problems) {
	process.stderr.write(`nightly-checks: ${problem}\n`);
}
process.stdout.write(`nightly-checks: ${releases.length} releases, ${tags.length} tags, ${problems.length} problems\n`);
process.exitCode = problems.length === 0 ? 0 : 1;
