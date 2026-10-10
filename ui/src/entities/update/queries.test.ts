import { describe, expect, it } from 'vitest';

import type { UpdateView } from '../../shared/ipc/bindings';
import { pollInterval } from './queries';

const VIEW: UpdateView = {
	current: '2.0.0',
	channel: 'stable',
	automatic: true,
	policy: null,
	state: 'idle',
	version: null,
	lastCheckAt: null,
	failedInstall: null,
	required: null,
};

describe('pollInterval', () => {
	it('polls while updates are not ready, checking or downloading', () => {
		expect(pollInterval(null)).toBe(2000);
		expect(pollInterval({ ...VIEW, state: 'checking' })).toBe(2000);
		expect(pollInterval({ ...VIEW, state: 'downloading' })).toBe(2000);
	});

	it('stops in every settled state', () => {
		for (const state of ['idle', 'upToDate', 'available', 'ready', 'failed'] as const) {
			expect(pollInterval({ ...VIEW, state })).toBe(false);
		}
		expect(pollInterval(undefined)).toBe(false);
	});
});
