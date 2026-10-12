import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';

const FIRST = '2.0.0';
const repo = process.env.GITHUB_REPOSITORY ?? '';
const sha = process.env.GITHUB_SHA ?? '';

const gh = (args) => execFileSync('gh', args, { encoding: 'utf8' });
const tryGh = (args) => {
	try {
		return gh(args);
	} catch {
		return undefined;
	}
};

const pending = tryGh([
	'api',
	`repos/${repo}/contents/package.json?ref=release-please--branches--main`,
	'--jq',
	'.content',
]);
const stable = gh(['api', '--paginate', `repos/${repo}/git/matching-refs/tags/v`, '--jq', '.[].ref'])
	.split('\n')
	.map((ref) => /^refs\/tags\/v(\d+)\.(\d+)\.(\d+)$/.exec(ref))
	.filter(Boolean)
	.map((match) => match.slice(1).map(Number))
	.sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2])
	.at(-1);

let core = FIRST;
if (pending !== undefined) {
	core = JSON.parse(Buffer.from(pending, 'base64').toString('utf8')).version;
} else if (stable !== undefined) {
	core = `${stable[0]}.${stable[1]}.${stable[2] + 1}`;
}
if (!/^\d+\.\d+\.\d+$/.test(core)) {
	process.stderr.write(`beta-tag: ${JSON.stringify(core)} is not a release version\n`);
	process.exit(1);
}

const prefix = `beta-v${core}-beta.`;
const taken = gh(['api', `repos/${repo}/git/matching-refs/tags/${prefix}`, '--jq', '.[].ref'])
	.split('\n')
	.filter(Boolean).length;
const tag = `${prefix}${taken + 1}`;

gh(['api', `repos/${repo}/git/refs`, '-f', `ref=refs/tags/${tag}`, '-f', `sha=${sha}`]);
gh([
	'release',
	'create',
	tag,
	'--repo',
	repo,
	'--draft',
	'--prerelease',
	'--verify-tag',
	'--title',
	tag,
	'--notes',
	'',
]);
process.stdout.write(`beta-tag: ${tag} at ${sha}\n`);
if (process.env.GITHUB_OUTPUT) {
	appendFileSync(process.env.GITHUB_OUTPUT, `tag=${tag}\n`);
}
