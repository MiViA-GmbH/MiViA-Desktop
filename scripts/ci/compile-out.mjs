import { execFileSync } from 'node:child_process';
import { readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const fail = (message) => {
	process.stderr.write(`compile-out: ${message}\n`);
	process.exit(1);
};

const tag = process.env.TAG ?? '';
const run = process.env.RUN ?? '';
const repo = process.env.REPO ?? '';
if (!/^(beta-)?v\d+\.\d+\.\d+(-beta\.\d+)?$/.test(tag)) {
	fail(`TAG ${JSON.stringify(tag)} is not a release tag`);
}
if (!/^\d+$/.test(run)) {
	fail(`RUN ${JSON.stringify(run)} is not a workflow run id`);
}
const version = tag.replace(/^beta-/, '').replace(/^v/, '');
const gh = (args) => execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }).trim();
const sha = gh(['api', `repos/${repo}/commits/${tag}`, '--jq', '.sha']);

rmSync('out', { recursive: true, force: true });
gh(['run', 'download', run, '--repo', repo, '--name', 'compile-out', '--dir', 'out']);

const expected = [
	'THIRD-PARTY-NOTICES.txt',
	'cargo-tauri.exe',
	`mivia-desktop-${version}-cargo.cdx.json`,
	`mivia-desktop-${version}-npm.cdx.json`,
	'mivia-desktop.exe',
	'package.json',
	'xtask.exe',
]
	.sort()
	.join(' ');
const actual = readdirSync('out').sort().join(' ');
if (actual !== expected) {
	fail(`run ${run} holds ${actual}, expected ${expected}`);
}

for (const name of readdirSync('out')) {
	gh([
		'attestation',
		'verify',
		join('out', name),
		'--repo',
		repo,
		'--signer-workflow',
		`${repo}/.github/workflows/build-release.yml`,
		'--source-ref',
		'refs/heads/main',
		'--source-digest',
		sha,
		'--deny-self-hosted-runners',
	]);
}
process.stdout.write(`compile-out: run ${run} is attested for ${tag} at ${sha}\n`);
