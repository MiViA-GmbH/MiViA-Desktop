import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const SNAPSHOT = 'crates/adapters/api/openapi.snapshot.json';
const SOURCE = 'https://app.mivia.ai/docs/json';

const sortKeys = (value) => {
	if (Array.isArray(value)) {
		return value.map(sortKeys);
	}
	if (value === null || typeof value !== 'object') {
		return value;
	}
	const sorted = {};
	for (const key of Object.keys(value).sort()) {
		sorted[key] = sortKeys(value[key]);
	}
	return sorted;
};

const normalise = (spec) => {
	const { servers: _servers, ...rest } = spec;
	return `${JSON.stringify(sortKeys({ ...rest, info: { ...rest.info, version: 'snapshot' } }), null, 2)}\n`;
};

const formatted = (text) => {
	const biome = 'node_modules/@biomejs/biome/bin/biome';
	const result = spawnSync(process.execPath, [biome, 'format', `--stdin-file-path=${SNAPSHOT}`], {
		input: text,
		encoding: 'utf8',
		maxBuffer: 64 * 1024 * 1024,
	});
	if (result.status !== 0) {
		throw new Error(`biome format failed: ${result.stderr}`);
	}
	return result.stdout;
};

const fetchSpec = async (url) => {
	const response = await fetch(url, {
		headers: { accept: 'application/json', 'user-agent': 'mivia-desktop-dev-tools' },
		redirect: 'error',
	});
	if (!response.ok) {
		throw new Error(`GET ${url} answered ${response.status}`);
	}
	return response.json();
};

const [command = 'write', input] = process.argv.slice(2);
const spec = input ? JSON.parse(readFileSync(input, 'utf8')) : await fetchSpec(SOURCE);
const text = formatted(normalise(spec));
if (command === 'check') {
	const same = readFileSync(SNAPSHOT, 'utf8') === text;
	process.stdout.write(same ? 'snapshot matches\n' : 'snapshot differs from the source\n');
	process.exitCode = same ? 0 : 1;
} else {
	writeFileSync(SNAPSHOT, text);
}
