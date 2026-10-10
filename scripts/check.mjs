import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

const windows = process.platform === 'win32';

const tool = (name) => {
	const local = join('.tools', 'bin', windows ? `${name}.exe` : name);
	return existsSync(local) ? local : name;
};

const localShellcheck = tool('shellcheck');

const ui = (script) => ['pnpm', ['--filter', '@mivia-desktop/ui', script]];

const steps = {
	fmt: ['cargo', ['fmt', '--all', '--check']],
	lint: ['cargo', ['lint']],
	'lint-tests': ['cargo', ['lint-tests']],
	'source-rules': ['cargo', ['xtask', 'source-rules']],
	arch: ['cargo', ['xtask', 'arch']],
	'command-size': ['cargo', ['xtask', 'command-size']],
	biome: ['pnpm', ['exec', 'biome', 'ci', '.']],
	tsc: ui('typecheck'),
	fallow: ui('fallow'),
	canary: ['cargo', ['xtask', 'canary']],
	deny: [tool('cargo-deny'), ['check']],
	'deny-fuzz': [tool('cargo-deny'), ['--manifest-path', 'fuzz/Cargo.toml', 'check']],
	'deny-perf': [tool('cargo-deny'), ['--manifest-path', 'perf/instr/Cargo.toml', 'check']],
	machete: [tool('cargo-machete'), []],
	jscpd: ['pnpm', ['exec', 'jscpd']],
	size: ui('size'),
	'ci-integrity': ['cargo', ['xtask', 'ci-integrity']],
	actionlint: [tool('actionlint'), localShellcheck === 'shellcheck' ? [] : ['-shellcheck', resolve(localShellcheck)]],
	zizmor: [tool('zizmor'), ['--persona', 'pedantic', '--offline', '.github']],
	test: ['cargo', ['test', '--workspace', '--locked']],
	audit: ['pnpm', ['audit', '--prod']],
	'fmt-write': ['cargo', ['fmt', '--all']],
	'biome-write': ['pnpm', ['exec', 'biome', 'check', '--write', '.']],
	'clippy-fix': ['cargo', ['clippy', '--fix', '--allow-dirty', '--workspace', '--lib', '--bins']],
};

const check = ['fmt', 'lint', 'lint-tests', 'source-rules', 'arch', 'command-size', 'biome', 'tsc', 'fallow'];
const groups = {
	check,
	full: [
		...check,
		'canary',
		'deny',
		'deny-fuzz',
		'deny-perf',
		'machete',
		'jscpd',
		'size',
		'ci-integrity',
		'actionlint',
		'zizmor',
		'test',
		'audit',
	],
	fix: ['fmt-write', 'biome-write', 'clippy-fix'],
};

const run = (name) => {
	const [program, args] = steps[name];
	const started = Date.now();
	const result = spawnSync(program, args, { stdio: 'inherit' });
	const seconds = ((Date.now() - started) / 1000).toFixed(1);
	const passed = result.status === 0;
	process.stdout.write(`${passed ? 'ok    ' : 'FAILED'} ${name} (${seconds} s)\n`);
	return passed;
};

const selection = process.argv.slice(2);
const names = selection.flatMap((entry) => groups[entry] ?? [entry]);
const unknown = names.filter((name) => !(name in steps));
if (unknown.length > 0 || names.length === 0) {
	process.stderr.write(`usage: node scripts/check.mjs <${Object.keys(groups).join('|')}|step...>\n`);
	process.exit(2);
}
const failed = names.filter((name) => !run(name));
process.stdout.write(failed.length === 0 ? 'all checks passed\n' : `failed: ${failed.join(', ')}\n`);
process.exit(failed.length === 0 ? 0 : 1);
