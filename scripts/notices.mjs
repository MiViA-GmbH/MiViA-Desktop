import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const windows = process.platform === 'win32';
const work = join('target', 'notices');
const output = process.argv[2] ?? join('target', 'debug', 'THIRD-PARTY-NOTICES.txt');
const heavy = '='.repeat(78);
const light = '-'.repeat(78);
const licenceFile = /^(licen[cs]e|copying|notice)/i;

const fail = (message) => {
	process.stderr.write(`notices: ${message}\n`);
	process.exit(1);
};

const tool = (name) => {
	const local = join('.tools', 'bin', windows ? `${name}.exe` : name);
	return existsSync(local) ? local : name;
};

const capture = (program, args) => {
	const result = spawnSync(program, args, { encoding: 'utf8', shell: windows, maxBuffer: 1 << 28 });
	if (result.status !== 0) {
		fail(`${program} ${args.join(' ')} failed\n${result.stderr}`);
	}
	return result.stdout;
};

const read = (path) => readFileSync(path, 'utf8').replaceAll('\r\n', '\n').trim();

function aboutConfig() {
	const lists = read(join('quality', 'licences.toml')).matchAll(/allow = \[([^\]]*)\]/g);
	const accepted = [...new Set([...lists].flatMap((list) => [...list[1].matchAll(/"([^"]+)"/g)].map((id) => id[1])))];
	const path = join(work, 'about.toml');
	const lines = [
		`accepted = ${JSON.stringify(accepted)}`,
		'targets = ["x86_64-pc-windows-msvc"]',
		'ignore-build-dependencies = true',
		'ignore-dev-dependencies = true',
		'private = { ignore = true }',
	];
	writeFileSync(path, `${lines.join('\n')}\n`);
	return path;
}

function crateName({ crate }, id) {
	const name = `${crate.name} ${crate.version}`;
	return id === 'MPL-2.0' ? `${name} (source: https://crates.io/crates/${crate.name}/${crate.version})` : name;
}

function rustGroups() {
	const json = join(work, 'about.json');
	const manifest = join('src-tauri', 'Cargo.toml');
	const args = ['generate', '-c', aboutConfig(), '-m', manifest, '--features', 'custom-protocol', '--locked', '--fail'];
	capture(tool('cargo-about'), [...args, '--format', 'json', '-o', json]);
	const about = JSON.parse(readFileSync(json, 'utf8'));
	const groups = about.licenses.map((licence) => ({
		id: licence.id,
		users: licence.used_by.map((user) => crateName(user, licence.id)),
		text: licence.text.trim(),
	}));
	const listed = new Set(about.licenses.flatMap((licence) => licence.used_by.map((user) => user.crate.id)));
	const missing = about.crates.filter((entry) => !listed.has(entry.package.id));
	if (missing.length > 0) {
		fail(`crates without a licence text: ${missing.map((entry) => entry.package.id).join(', ')}`);
	}
	return { groups, crates: about.crates.map((entry) => entry.package.name) };
}

function packageText(dir, id, manifest) {
	const files = readdirSync(dir)
		.filter((name) => licenceFile.test(name))
		.sort();
	if (files.length > 0) {
		return files.map((name) => read(join(dir, name))).join('\n\n');
	}
	const fallback = join('packaging', 'notices', `${id}.txt`);
	if (!existsSync(fallback)) {
		fail(`${manifest.name} ${manifest.version} has no licence file and ${fallback} does not exist`);
	}
	const author = manifest.author?.name ?? manifest.author ?? `the ${manifest.name} authors`;
	return `Copyright (c) ${author}\n\n${read(fallback)}`;
}

function uiGroups() {
	const args = ['--filter', '@mivia-desktop/ui...', 'licenses', 'list', '--prod', '--json'];
	const listed = JSON.parse(capture('pnpm', args));
	const groups = new Map();
	for (const [id, packages] of Object.entries(listed)) {
		for (const dir of packages.flatMap((entry) => entry.paths)) {
			const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
			const text = packageText(dir, id, manifest);
			const key = `${id}\n${text}`;
			const group = groups.get(key) ?? { id, users: [], text };
			group.users.push(`${manifest.name} ${manifest.version}`);
			groups.set(key, group);
		}
	}
	return [...groups.values()];
}

const section = (title, body) => `${heavy}\n${title}\n${heavy}\n\n${body}\n`;

const listing = (groups) =>
	groups
		.map((group) => `${light}\n${group.id}\nUsed by: ${group.users.sort().join(', ')}\n${light}\n\n${group.text}\n`)
		.join('\n');

function checkPackaging(crates, other) {
	const release = JSON.parse(readFileSync(join('src-tauri', 'tauri.release.conf.json'), 'utf8'));
	if (!Object.values(release.bundle.resources ?? {}).includes('THIRD-PARTY-NOTICES.txt')) {
		fail('src-tauri/tauri.release.conf.json does not install THIRD-PARTY-NOTICES.txt');
	}
	if (crates.includes('jpeg-encoder') && !other.includes('Independent JPEG Group')) {
		fail('jpeg-encoder ships but packaging/notices/other.txt lacks the IJG sentence');
	}
}

mkdirSync(work, { recursive: true });
const rust = rustGroups();
const other = read(join('packaging', 'notices', 'other.txt'));
checkPackaging(rust.crates, other);
const intro =
	'MiViA Desktop includes the third-party software listed below. Each entry names its licence and the full text.';
const text = [
	`MiViA Desktop third-party notices\n\n${intro}\n`,
	section('Other components', other),
	section('Rust crates', listing(rust.groups)),
	section('User interface packages', listing(uiGroups())),
].join('\n');
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, text);
process.stdout.write(`notices: wrote ${output}\n`);
