import type { Icon } from '@tabler/icons-react';

export type Tone = 'done' | 'working' | 'waiting' | 'problem' | 'off';

export const TONE_COLOR: Record<Tone, string> = {
	done: 'forest',
	working: 'jade',
	waiting: 'orange',
	problem: 'red',
	off: 'stone',
};

export type Status = { tone: Tone; icon: Icon; label: string };
