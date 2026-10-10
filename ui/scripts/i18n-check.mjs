import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const problems = [];
const report = (rule, message) => problems.push(`rule ${rule}: ${message}`);

function flatten(value, prefix = '') {
	return Object.entries(value).flatMap(([key, inner]) =>
		typeof inner === 'object' ? flatten(inner, `${prefix}${key}.`) : [[`${prefix}${key}`, inner]],
	);
}

function load(dir, language) {
	const file = join(root, dir, `${language}.json`);
	return existsSync(file) ? new Map(flatten(JSON.parse(readFileSync(file, 'utf8')))) : new Map();
}

function sources(dir, pattern) {
	const path = join(root, dir);
	if (!existsSync(path)) {
		return [];
	}
	if (statSync(path).isFile()) {
		return pattern.test(path) && !path.endsWith('.test.ts') ? [[path, readFileSync(path, 'utf8')]] : [];
	}
	return readdirSync(path).flatMap((name) => sources(join(dir, name), pattern));
}

const stripPlural = (key) => key.replace(/_(one|other)$/, '');
const placeholders = (text) =>
	[...text.matchAll(/\{\{(\w+)\}\}|%\{(\w+)\}/g)]
		.map((m) => m[1] ?? m[2])
		.sort()
		.join();
const withoutPlaceholders = (text) => text.replace(/\{\{\w+\}\}|%\{\w+\}/g, '');
const neverShown = readFileSync(join(root, 'CONTEXT.md'), 'utf8')
	.match(/^Never shown: (.*?)\. /m)[1]
	.split(', ')
	.concat(['Job', 'Batch', 'Hash', 'Fingerprint']);

const pairRules = [
	[1, ({ en, de }) => en === undefined || de === undefined, 'is missing in en or de'],
	[1, ({ en, de }) => en === '' || de === '', 'is empty'],
	[2, ({ en, de }) => placeholders(en ?? '') !== placeholders(de ?? ''), 'has different placeholders in en and de'],
	[2, ({ en, de, braces }) => `${en}${de}`.includes(braces), 'uses the placeholder style of the other file'],
];

const keyRules = [
	[5, ({ key, keys }) => key.endsWith('_one') && !keys.has(`${stripPlural(key)}_other`), 'has no _other'],
	[5, ({ key, keys }) => key.endsWith('_other') && !keys.has(`${stripPlural(key)}_one`), 'has no _one'],
	[5, ({ key }) => key !== stripPlural(key) && key.startsWith('desktop.native.'), 'is a plural under desktop.native'],
	[
		5,
		({ key, list }) => key.includes(':') || list.some((other) => other.startsWith(`${key}.`)),
		'has : or prefixes a key',
	],
	[5, ({ key }) => key.split('.').length > 4, 'has more than 4 segments'],
];

const valueRules = [
	[6, ({ text }) => /['"]/.test(text), 'uses an ASCII quote'],
	[6, ({ text, language }) => language === 'de' && /\b(du|dich|dir|dein\w*)\b/i.test(text), 'uses du'],
	[6, ({ text }) => neverShown.some((word) => new RegExp(`\\b${word}\\b`, 'i').test(text)), 'uses a never-shown word'],
	[6, ({ key, value }) => key.startsWith('desktop.native.') && value.length > 30, 'is longer than 30 characters'],
	[6, ({ key, value }) => key.endsWith('.sendToName') && !value.startsWith('MiViA'), 'does not start with MiViA'],
];

function apply(rules, subject, label) {
	for (const [rule, failing, message] of rules) {
		if (failing(subject)) {
			report(rule, `${label} ${message}`);
		}
	}
}

function checkPairs(en, de, braces) {
	for (const key of new Set([...en.keys(), ...de.keys()])) {
		apply(pairRules, { en: en.get(key), de: de.get(key), braces }, key);
	}
}

function checkKeys(all) {
	const list = [...all.keys()];
	const keys = new Set(list);
	for (const key of list) {
		apply(keyRules, { key, keys, list }, key);
	}
}

function checkValues(language, values) {
	for (const [key, value] of values) {
		apply(valueRules, { key, value, language, text: withoutPlaceholders(value) }, `${key} (${language})`);
	}
}

function usages(ui, rust) {
	const found = (files, pattern) =>
		files.flatMap(([file, text]) => [...text.matchAll(pattern)].map((m) => [m[1], file]));
	return new Map([...found(ui, /['"](desktop\.[\w.]+)['"]/g), ...found(rust, /\bt!\("(desktop\.[\w.]+)"/g)]);
}

function checkLiterals(ui, rust) {
	for (const [file, text] of ui) {
		if (/`desktop\./.test(text) && !file.includes(join('entities', 'model'))) {
			report(3, `${relative(root, file)} builds a key from a template literal`);
		}
	}
	for (const [file, text] of rust) {
		if (/\bt!\(\s*[^"\s]/.test(text)) {
			report(3, `${relative(root, file)} calls t! with a non-literal key`);
		}
	}
}

function checkUsage(all, ui, rust) {
	const used = usages(ui, rust);
	const known = new Set([...all.keys()].map(stripPlural));
	for (const [key, file] of used) {
		if (!known.has(key)) {
			report(3, `${key} used in ${relative(root, file)} is missing in en`);
		}
	}
	for (const key of known) {
		const file = used.get(key) ?? '';
		const owner = key.startsWith('desktop.native.') ? '.rs' : '.ts';
		if (!file.includes(owner)) {
			report(4, `${key} is not used by its owner`);
		}
	}
	checkLiterals(ui, rust);
}

const ui = [['ui/src/shared/i18n/locales/desktop', 'ui/src/shared/i18n/locales/setup'], '%{'];
const rust = ['src-tauri/locales', '{{'];
const loadAll = (dirs, language) => new Map(dirs.flatMap((dir) => [...load(dir, language)]));
const [en, de, rustEn, rustDe] = [loadAll(ui[0], 'en'), loadAll(ui[0], 'de'), load(rust[0], 'en'), load(rust[0], 'de')];
checkPairs(en, de, ui[1]);
checkPairs(rustEn, rustDe, rust[1]);
const all = new Map([...en, ...rustEn]);
checkKeys(all);
checkValues('en', all);
checkValues('de', new Map([...de, ...rustDe]));
checkUsage(all, sources('ui/src', /\.tsx?$/), sources('src-tauri/src', /\.rs$/));

process.stdout.write(problems.length === 0 ? `i18n: ${all.size} keys ok\n` : `${problems.join('\n')}\n`);
process.exit(problems.length === 0 ? 0 : 1);
