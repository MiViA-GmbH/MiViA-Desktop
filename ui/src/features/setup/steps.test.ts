import { describe, expect, it } from 'vitest';

import type { SetupView } from '../../shared/ipc/bindings';
import { countedSteps, isLast, isStep, nextStep, previousStep, stepperSteps } from './steps';

function view(changes: Partial<SetupView> = {}): SetupView {
	return {
		completed: false,
		step: 'welcome',
		resumed: false,
		host: 'app.mivia.ai',
		supportContact: 'support@mivia.ai',
		detectedLanguage: 'English',
		locks: { language: false, legacySearch: true, autostart: null, explorer: null, telemetry: null },
		legacy: null,
		legacyFolderMissing: null,
		connection: null,
		folder: null,
		preset: 'byHand',
		report: null,
		models: [],
		options: ['autostart', 'explorer', 'telemetry'],
		imagesSent: 0,
		...changes,
	};
}

const legacy = {
	folder: 'C:\\Users\\lab\\Downloads\\MiViA',
	inputFolder: null,
	modelCount: 0,
	flags: [],
	take: null,
} satisfies SetupView['legacy'];

describe('wizard steps', () => {
	it('walks every step in order and counts the shown ones', () => {
		const plain = view();
		expect(stepperSteps(plain)).toEqual(['key', 'folder', 'series', 'models', 'finish']);
		expect(nextStep(plain, 'welcome')).toBe('key');
		expect(nextStep(plain, 'finish')).toBe('done');
		expect(previousStep(plain, 'key')).toBe('welcome');
		expect(countedSteps(plain)).toHaveLength(5);
	});

	it('inserts the old version step only when an old app was found and the search is allowed', () => {
		expect(nextStep(view({ legacy }), 'welcome')).toBe('legacy');
		const locked = view({ legacy, locks: { ...view().locks, legacySearch: false } });
		expect(nextStep(locked, 'welcome')).toBe('key');
	});

	it('skips series and models after a folder skip and counts them out', () => {
		const skipped = view({ options: ['skipFolder'] });
		expect(nextStep(skipped, 'folder')).toBe('finish');
		expect(previousStep(skipped, 'finish')).toBe('folder');
		expect(countedSteps(skipped)).toEqual(['key', 'folder', 'finish']);
		expect(stepperSteps(skipped)).toContain('series');
	});

	it('hides the last step when IT sets all three of its choices', () => {
		const locked = view({ locks: { ...view().locks, autostart: true, explorer: false, telemetry: false } });
		expect(isLast(locked, 'models')).toBe(true);
		expect(stepperSteps(locked)).not.toContain('finish');
	});

	it('accepts only known step names from the route', () => {
		expect(isStep('models')).toBe(true);
		expect(isStep('done')).toBe(true);
		expect(isStep('reports')).toBe(false);
	});
});
