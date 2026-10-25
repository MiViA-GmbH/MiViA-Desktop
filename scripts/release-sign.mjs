import { spawnSync } from 'node:child_process';
import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const THUMBPRINT = '9104B16C1523CEBB0A3605EA176330DD30587AEE';
const TIMESTAMP = 'http://time.certum.pl';
const KITS = 'C:\\Program Files (x86)\\Windows Kits\\10\\bin';
const work = join('target', 'release-sign');
const nsis = join('target', 'release', 'bundle', 'nsis');

const fail = (message) => {
	process.stderr.write(`release-sign: ${message}\n`);
	process.exit(1);
};

const run = (program, args, options = {}) => {
	const result = spawnSync(program, args, { stdio: 'inherit', ...options });
	if (result.status !== 0) {
		fail(`${program} ${args.join(' ')} failed`);
	}
};

const read = (program, args) => {
	const result = spawnSync(program, args, { encoding: 'utf8' });
	if (result.status !== 0) {
		fail(`${program} ${args.join(' ')} failed: ${result.stderr.trim()}`);
	}
	return result.stdout.trim();
};

const signtool = () => {
	if (!existsSync(KITS)) {
		fail('the Windows SDK is not installed');
	}
	const newest = readdirSync(KITS)
		.filter((name) => /^10\.\d+\.\d+\.\d+$/.test(name))
		.map((name) => join(KITS, name, 'x64', 'signtool.exe'))
		.filter((path) => existsSync(path))
		.sort((a, b) => a.localeCompare(b, 'en', { numeric: true }))
		.pop();
	return newest ?? fail('signtool.exe not found, install the Windows SDK');
};

const args = process.argv.slice(2);
const [tag = ''] = args;
const option = (name) => {
	const at = args.indexOf(name);
	if (at < 0) {
		return undefined;
	}
	return args[at + 1] ?? fail(`${name} needs a value`);
};
const from = option('--from');
const dryRun = args.includes('--dry-run') || from !== undefined;
if (!/^(beta-)?v\d+\.\d+\.\d+(-beta\.\d+)?$/.test(tag)) {
	fail('usage: pnpm release:sign <tag> [--dry-run] [--from <compile-out dir>]');
}
if (process.platform !== 'win32') {
	fail('the code signing card is read on Windows only');
}

if (read('git', ['status', '--porcelain']) !== '') {
	fail('the checkout is not clean');
}
const head = read('git', ['rev-parse', 'HEAD']);
const repo = read('gh', ['repo', 'view', '--json', 'nameWithOwner', '--jq', '.nameWithOwner']);
let compileRun = '';
if (from === undefined) {
	const sha = read('gh', ['api', `repos/${repo}/commits/${tag}`, '--jq', '.sha']);
	if (sha !== head) {
		fail(`HEAD is ${head}, ${tag} is ${sha}; check out the tag first`);
	}
	const runs = JSON.parse(
		read('gh', [
			'api',
			`repos/${repo}/actions/artifacts?name=compile-out&per_page=100`,
			'--jq',
			`[.artifacts[] | select(.expired | not) | select(.workflow_run.head_sha == "${sha}") | .workflow_run.id]`,
		]),
	);
	compileRun = String(runs[0] ?? fail(`no compile output for ${sha}; it expires 7 days after the build`));
	run(process.execPath, ['scripts/ci/compile-out.mjs'], {
		env: { ...process.env, TAG: tag, RUN: compileRun, REPO: repo },
	});
} else {
	rmSync('out', { recursive: true, force: true });
	cpSync(from, 'out', { recursive: true });
	process.stdout.write(`release-sign: ${from} is a local compile output, nothing is uploaded\n`);
}

const tool = signtool();
const overlay = resolve(work, 'tauri.sign.conf.json');
mkdirSync(work, { recursive: true });
writeFileSync(
	overlay,
	JSON.stringify({
		bundle: {
			windows: { signCommand: { cmd: resolve('out', 'xtask.exe'), args: ['authenticode', 'sign', '%1'] } },
		},
	}),
);
mkdirSync(join('target', 'release'), { recursive: true });
copyFileSync(join('out', 'mivia-desktop.exe'), join('target', 'release', 'mivia-desktop.exe'));
copyFileSync(join('out', 'THIRD-PARTY-NOTICES.txt'), join('target', 'release', 'THIRD-PARTY-NOTICES.txt'));
copyFileSync(join('out', 'package.json'), 'package.json');
rmSync(nsis, { recursive: true, force: true });

process.stdout.write(
	'release-sign: expect 4 card PIN dialogs (the app, all NSIS plugins at once, the uninstaller, the installer); cancelling one stops the run\n',
);
const bundled = spawnSync(
	join('out', 'cargo-tauri.exe'),
	['bundle', '--bundles', 'nsis', '--config', 'src-tauri/tauri.release.conf.json', '--config', overlay],
	{
		stdio: 'inherit',
		env: { ...process.env, RELEASE_SIGNTOOL: tool, RELEASE_THUMBPRINT: THUMBPRINT, RELEASE_TIMESTAMP: TIMESTAMP },
	},
);
run('git', ['checkout', '--', 'package.json']);
if (bundled.status !== 0) {
	fail('bundling or signing failed; nothing is retried, start again when the cause is fixed');
}

const installers = readdirSync(nsis).filter((name) => name.endsWith('-setup.exe'));
if (installers.length !== 1) {
	fail(`expected one installer in ${nsis}, found ${installers.length}`);
}
const installer = join(nsis, installers[0]);
const extract = join(work, 'extract');
rmSync(extract, { recursive: true, force: true });
run('7z', ['x', '-y', `-o${extract}`, installer, 'mivia-desktop.exe', '$PLUGINSDIR'], { stdio: 'ignore' });
const plugins = readdirSync(join(extract, '$PLUGINSDIR'))
	.filter((name) => name.endsWith('.dll'))
	.map((name) => join(extract, '$PLUGINSDIR', name));
const signed = [installer, join(extract, 'mivia-desktop.exe'), ...plugins];

run(tool, ['verify', '/pa', '/tw', ...signed]);
run(join('out', 'xtask.exe'), ['authenticode', 'verify', ...signed]);
run(join('out', 'xtask.exe'), [
	'authenticode',
	'same-app',
	join(extract, 'mivia-desktop.exe'),
	join('out', 'mivia-desktop.exe'),
]);
run('powershell', [
	'-NoProfile',
	'-Command',
	`Get-AuthenticodeSignature -LiteralPath '${resolve(installer)}','${resolve(extract, 'mivia-desktop.exe')}' | Format-Table Status,@{n='Signer';e={$_.SignerCertificate.Subject}},Path -AutoSize -Wrap`,
]);

if (dryRun) {
	process.stdout.write(`release-sign: dry run, ${installer} stays local\n`);
	process.exit(0);
}
run('gh', ['release', 'upload', tag, installer, '--repo', repo]);
run('gh', [
	'workflow',
	'run',
	'signed-release.yml',
	'--repo',
	repo,
	'--ref',
	'main',
	'-f',
	`tag=${tag}`,
	'-f',
	`compile_run=${compileRun}`,
]);
process.stdout.write(`release-sign: ${installers[0]} uploaded to ${tag}, signed-release started\n`);
