import type { PendingReportView, PolicyRow, ProblemReportPreview, SupportView } from '../../shared/ipc/bindings';

const NAMES = [
	'ServerUrl',
	'Language',
	'Autostart',
	'ExplorerIntegration',
	'Telemetry',
	'IssueReporting',
	'LegacyMigration',
	'DefaultInputFolder',
	'SupportContact',
	'AutoUpdate',
	'ProxyUrl',
	'MaxUploadConcurrency',
	'MaxUploadMbps',
	'DisableWatchMarker',
];

const SET: Record<string, Omit<PolicyRow, 'name'>> = {
	Telemetry: { value: 'off', source: 'machine' },
	SupportContact: { value: 'mailto:it-service@kessler-werkstoff.de', source: 'machine' },
	MaxUploadMbps: { value: '20', source: 'user' },
};

const SUPPORT: SupportView = {
	reporting: 'on',
	canSend: true,
	contact: 'mailto:it-service@kessler-werkstoff.de',
	policies: NAMES.map((name) => ({ name, value: null, source: null, ...SET[name] })),
	version: '2.0.0',
	sessionId: 'mdt-0192f1c4-7a10-7b31-9c55-3e1f0a6b2d11',
	installId: '5f0c2a71-9e4b-4d2a-8c6f-1b7e3d9a0c42',
	pending: 1,
};

const PENDING: PendingReportView[] = [
	{
		reportId: '0192f1d0-1111-7000-8000-000000000001',
		reference: '0192f1d0',
		createdAt: '2026-10-12T07:40:00.000Z',
		state: 'saveAndEmail',
		attempts: 1,
	},
];

const PREVIEW: ProblemReportPreview = {
	reportId: '0192f1d0-2222-7000-8000-000000000002',
	reference: '0192f1d0',
	entries: [
		{ name: 'manifest.json', bytes: 912, lines: 31 },
		{ name: 'system.json', bytes: 318, lines: 9 },
		{ name: 'settings.json', bytes: 604, lines: 22 },
		{ name: 'errors.json', bytes: 1840, lines: 48 },
		{ name: 'logs/mivia.log.jsonl', bytes: 2_412_544, lines: 9184 },
		{ name: 'logs/memory-tail.jsonl', bytes: 286_720, lines: 1000 },
	],
	totalBytes: 431_104,
	truncated: false,
	from: '2026-10-09T08:30:00.000Z',
	to: '2026-10-12T08:30:00.000Z',
	sessionIds: ['mdt-0192f1c4-7a10-7b31-9c55-3e1f0a6b2d11', 'mdt-0192e8aa-0c11-7d40-8a77-2f4b6c1d9e02'],
	installId: SUPPORT.installId,
	firstErrors: ['internal:-:save_report'],
};

const FILE = '{"os":"Windows 11 23H2","arch":"x86_64","locale":"de-DE","timeZone":"Europe/Berlin","version":"2.0.0"}';

export function supportResponses(scenario: string): Record<string, unknown> {
	const reporting = scenario === 'reporting-off' ? 'off' : 'on';
	return {
		get_support: { ...SUPPORT, reporting },
		list_pending_reports: PENDING,
		prepare_problem_report: PREVIEW,
		read_problem_report_file: FILE,
		claim_report_offer: false,
		delete_local_logs: 3,
	};
}
