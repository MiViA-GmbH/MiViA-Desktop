import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const isObject = (value) => typeof value === 'object' && value !== null && !Array.isArray(value);

function leaves(value, prefix = '') {
	return Object.entries(value).flatMap(([key, inner]) =>
		isObject(inner) ? leaves(inner, `${prefix}${key}.`) : [`${prefix}${key}`],
	);
}

export function sorted(value) {
	if (!isObject(value)) {
		return value;
	}
	return Object.fromEntries(
		Object.keys(value)
			.sort()
			.map((key) => [key, sorted(value[key])]),
	);
}

function updated(local, remote) {
	return Object.fromEntries(
		Object.entries(local).map(([key, text]) => {
			const fresh = isObject(remote) ? remote[key] : undefined;
			if (isObject(text)) {
				return [key, updated(text, fresh)];
			}
			return [key, typeof fresh === 'string' ? fresh : text];
		}),
	);
}

export function pull(files, remote) {
	if (!isObject(remote.desktop)) {
		throw new Error('the language file has no desktop strings');
	}
	const next = Object.fromEntries(
		Object.entries(files).map(([name, file]) => [name, sorted({ desktop: updated(file.desktop, remote.desktop) })]),
	);
	const repoKeys = new Set(Object.values(files).flatMap((file) => leaves(file)));
	const remoteKeys = leaves({ desktop: remote.desktop });
	const remoteSet = new Set(remoteKeys);
	return {
		files: next,
		onlyRemote: remoteKeys.filter((key) => !repoKeys.has(key)).sort(),
		onlyRepo: [...repoKeys].filter((key) => !remoteSet.has(key)).sort(),
	};
}

export function modelLabels(remote) {
	if (!isObject(remote.model) || Object.keys(remote.model).length === 0) {
		throw new Error('the language file has no model labels');
	}
	return sorted({ model: remote.model });
}

async function fetchLanguage(language) {
	const response = await fetch(`https://app.mivia.ai/api/languages/${language}`);
	if (!response.ok) {
		throw new Error(`${language}: HTTP ${response.status}`);
	}
	return response.json();
}

const read = (path) => JSON.parse(readFileSync(join(root, path), 'utf8'));
const write = (path, value) => writeFileSync(join(root, path), `${JSON.stringify(value, null, '\t')}\n`);
function list(title, keys) {
	if (keys.length === 0) {
		return [];
	}
	return [`${title}:`, '', ...keys.map((key) => `- \`${key}\``), ''];
}

async function main() {
	const body = ['Texts pulled from Tolgee through the public language files of app.mivia.ai.', ''];
	let missing = false;
	for (const language of ['en', 'de']) {
		const remote = await fetchLanguage(language);
		const paths = {
			desktop: `ui/src/shared/i18n/locales/desktop/${language}.json`,
			setup: `ui/src/shared/i18n/locales/setup/${language}.json`,
		};
		const { files, onlyRemote, onlyRepo } = pull({ desktop: read(paths.desktop), setup: read(paths.setup) }, remote);
		write(paths.desktop, files.desktop);
		write(paths.setup, files.setup);
		write(`crates/adapters/server/labels/core.${language}.json`, modelLabels(remote));
		body.push(
			...list(`Keys only in Tolgee (${language})`, onlyRemote),
			...list(`Keys only in the repo (${language})`, onlyRepo),
		);
		missing ||= onlyRepo.length > 0;
	}
	process.stdout.write(`${body.join('\n')}\n`);
	process.exitCode = missing ? 2 : 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	await main();
}
