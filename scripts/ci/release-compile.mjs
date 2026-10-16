import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
	appendFileSync,
	copyFileSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	renameSync,
	rmSync,
	writeFileSync,
} from 'node:fs';
import { delimiter, join, resolve } from 'node:path';

const windows = process.platform === 'win32';
const exe = windows ? '.exe' : '';
const tools = join('target', 'release-tools');
const out = 'out';

const run = (program, args) => {
	const result = spawnSync(program, args, { stdio: 'inherit', shell: windows });
	if (result.status !== 0) {
		process.exit(result.status ?? 1);
	}
};

const fail = (message) => {
	process.stderr.write(`release-compile: ${message}\n`);
	process.exit(1);
};

const tag = process.env.TAG ?? '';
if (!/^(beta-)?v\d+\.\d+\.\d+(-beta\.\d+)?$/.test(tag)) {
	fail(`TAG ${JSON.stringify(tag)} is not a release tag`);
}
const version = tag.replace(/^beta-/, '').replace(/^v/, '');
const beta = tag.startsWith('beta-');
if (beta !== version.includes('-beta.')) {
	fail(`${tag} mixes the beta and stable tag forms`);
}

const manifest = JSON.parse(readFileSync('package.json', 'utf8'));
if (manifest.version !== version) {
	if (!beta) {
		fail(`package.json has ${manifest.version}, the tag says ${version}`);
	}
	manifest.version = version;
	writeFileSync('package.json', `${JSON.stringify(manifest, null, '\t')}\n`);
}
const tauriCli = manifest.devDependencies['@tauri-apps/cli'];

run('pnpm', ['install', '--frozen-lockfile']);
run('pnpm', ['--filter', '@mivia-desktop/ui', 'build']);
run('cargo', ['build', '--release', '--locked', '--frozen', '-p', 'mivia-desktop', '--features', 'custom-protocol']);
run('cargo', ['build', '--release', '--locked', '--frozen', '-p', 'xtask']);
run('cargo', ['install', 'tauri-cli', '--version', `=${tauriCli}`, '--locked', '--root', tools]);
run('cargo', ['install', 'cargo-cyclonedx', '--version', '=0.5.9', '--locked', '--root', tools]);
run('cargo', ['install', 'cargo-about', '--version', '=0.9.2', '--locked', '--features', 'cli', '--root', tools]);

mkdirSync(out, { recursive: true });
process.env.PATH = `${resolve(tools, 'bin')}${delimiter}${process.env.PATH}`;
run('node', ['scripts/notices.mjs', join(out, 'THIRD-PARTY-NOTICES.txt')]);
run(join(tools, 'bin', `cargo-cyclonedx${exe}`), [
	'cyclonedx',
	'--manifest-path',
	join('src-tauri', 'Cargo.toml'),
	'--format',
	'json',
	'--spec-version',
	'1.5',
	'--target',
	'x86_64-pc-windows-msvc',
	'--features',
	'custom-protocol',
	'--override-filename',
	'cargo.cdx',
]);
renameSync(join('src-tauri', 'cargo.cdx.json'), join(out, `mivia-desktop-${version}-cargo.cdx.json`));
const strays = execFileSync('git', ['ls-files', '--others', '--exclude-standard', '--', '*cargo.cdx.json'], {
	encoding: 'utf8',
});
for (const stray of strays.split('\n').filter(Boolean)) {
	rmSync(stray);
}
run('pnpm', [
	'--filter',
	'@mivia-desktop/ui',
	'sbom',
	'--sbom-format',
	'cyclonedx',
	'--prod',
	'--sbom-type',
	'application',
	'--out',
	resolve(out, `mivia-desktop-${version}-npm.cdx.json`),
]);

copyFileSync(join('target', 'release', `mivia-desktop${exe}`), join(out, `mivia-desktop${exe}`));
copyFileSync(join('target', 'release', `xtask${exe}`), join(out, `xtask${exe}`));
copyFileSync(join(tools, 'bin', `cargo-tauri${exe}`), join(out, `cargo-tauri${exe}`));
copyFileSync('package.json', join(out, 'package.json'));

const digests = readdirSync(out)
	.sort()
	.map(
		(name) =>
			`${createHash('sha256')
				.update(readFileSync(join(out, name)))
				.digest('hex')}  ${name}`,
	)
	.join('\n');
process.stdout.write(`${digests}\n`);
if (process.env.GITHUB_OUTPUT) {
	appendFileSync(process.env.GITHUB_OUTPUT, `digests<<DIGESTS\n${digests}\nDIGESTS\n`);
}
