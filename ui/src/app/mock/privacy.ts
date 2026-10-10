import type { PrivacyView, SentBatchView } from '../../shared/ipc/bindings';

const INSTALL = '5f0c2a71-9e4b-4d2a-8c6f-1b7e3d9a0c42';
const MACHINE = '3c9f01d2a7e84b65b0c1d2e3f4a5b6c7';

const SENT: SentBatchView[] = [
	{
		batchId: '0192f1d0-3333-4000-8000-000000000001',
		sentAt: '2026-10-12T08:12:00.000Z',
		events: 2,
		json: JSON.stringify(
			[
				{
					seq: 41,
					ts: '2026-10-12T08:10:12.402Z',
					name: 'folder.paused',
					v: 1,
					category: 'usage',
					accountMatch: true,
					fields: { folder_id: '7a4e5d21-2c1b-4f0a-9e63-5b8d1c0f2a99' },
				},
				{
					seq: 42,
					ts: '2026-10-12T08:11:48.100Z',
					name: 'ui.view',
					v: 1,
					category: 'usage',
					accountMatch: true,
					fields: { state: 'history' },
				},
			],
			null,
			2,
		),
	},
];

export function privacyResponses(scenario: string): Record<string, (args: Record<string, unknown>) => unknown> {
	const locked = scenario === 'privacy-locked';
	const state: PrivacyView = { enabled: !locked, locked, installId: INSTALL, machineId: locked ? null : MACHINE };
	return {
		get_privacy: () => state,
		set_telemetry: (args) => {
			state.enabled = args.enabled === true;
			return state;
		},
		list_sent_telemetry: () => (scenario === 'privacy-empty' ? [] : SENT),
		note_privacy_viewed: () => null,
	};
}
