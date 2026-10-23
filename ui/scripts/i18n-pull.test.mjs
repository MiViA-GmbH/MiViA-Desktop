import { describe, expect, it } from 'vitest';
import { modelLabels, pull } from './i18n-pull.mjs';

describe('pull', () => {
	const repo = {
		desktop: { desktop: { nav: { overview: 'Overview', old: 'Old' }, about: { title: 'About' } } },
		setup: { desktop: { wizard: { next: 'Next' } } },
	};
	const remote = {
		desktop: {
			nav: { overview: 'Start', fresh: 'New in Tolgee', old: { nested: 'x' } },
			about: { title: 'About MiViA' },
			wizard: { next: 'Continue' },
		},
		dashboard: { title: 'not ours' },
	};

	it('updates the texts of keys that exist in the repo and on the server', () => {
		const { files } = pull(repo, remote);
		expect(files.desktop.desktop.nav.overview).toBe('Start');
		expect(files.desktop.desktop.about.title).toBe('About MiViA');
		expect(files.setup.desktop.wizard.next).toBe('Continue');
	});

	it('never adds or removes a key and reports the differences', () => {
		const { files, onlyRemote, onlyRepo } = pull(repo, remote);
		expect(files.desktop.desktop.nav).toEqual({ old: 'Old', overview: 'Start' });
		expect(onlyRemote).toEqual(['desktop.nav.fresh', 'desktop.nav.old.nested']);
		expect(onlyRepo).toEqual(['desktop.nav.old']);
	});

	it('sorts the keys', () => {
		const { files } = pull(repo, remote);
		expect(Object.keys(files.desktop.desktop)).toEqual(['about', 'nav']);
		expect(Object.keys(files.desktop.desktop.nav)).toEqual(['old', 'overview']);
	});
});

describe('modelLabels', () => {
	it('keeps only the sorted model tree', () => {
		const labels = modelLabels({ model: { b: 'B', a: 'A', feedback: { MAG: 'Mag' } }, feedback: { MAG: 'Mag' } });
		expect(labels).toEqual({ model: { a: 'A', b: 'B', feedback: { MAG: 'Mag' } } });
		expect(Object.keys(labels.model)).toEqual(['a', 'b', 'feedback']);
	});

	it('refuses a file without models', () => {
		expect(() => modelLabels({ model: {} })).toThrow();
	});
});
