import type {
	CatalogueView,
	ConnectionNote,
	ConnectOutcome,
	DoneView,
	FolderPickView,
	IntegrationView,
	LegacyView,
	ReportFolderView,
	SetupInput,
	SetupView,
} from '../../shared/ipc/bindings';

const FOLDER: FolderPickView = {
	display: 'E:\\Werkstofflabor Kessler GmbH\\Mikroskop 1',
	name: 'Mikroskop 1',
	block: null,
	notes: [],
	existing: 42,
	existingMore: false,
};

const REPORT: ReportFolderView = {
	mode: 'subfolder',
	display: 'E:\\Werkstofflabor Kessler GmbH\\Mikroskop 1\\MiViA reports',
	subfolderName: 'MiViA reports',
	denied: null,
	longPath: false,
};

const LEGACY: LegacyView = {
	folder: 'C:\\Users\\moeller\\Downloads\\MiViA',
	inputFolder: 'E:\\Mikroskop\\Export',
	modelCount: 2,
	flags: ['hasKey'],
	take: null,
};

const CATALOGUE: CatalogueView = {
	loaded: true,
	models: [
		{ modelId: 'grain', name: 'Grain size', customizations: [{ id: 'c1', name: 'ASTM E112 lab' }] },
		{ modelId: 'phases', name: 'Phase fractions', customizations: [] },
		{ modelId: 'inclusions', name: 'Inclusions', customizations: [] },
	],
	preselected: [],
	legacyGone: [],
	legacyCustomGone: [],
};

export function setupView(scenario: string): SetupView {
	return {
		completed: !scenario.startsWith('setup'),
		step: 'welcome',
		resumed: scenario === 'setup-resume',
		host: 'app.mivia.ai',
		supportContact: 'support@mivia.ai',
		detectedLanguage: 'Deutsch',
		locks: { language: false, legacySearch: true, autostart: null, explorer: null, telemetry: null },
		legacy: scenario === 'setup-legacy' ? LEGACY : null,
		legacyFolderMissing: null,
		connection: null,
		folder: null,
		preset: 'byHand',
		report: null,
		models: [],
		options: ['autostart', 'explorer', 'legacyStop'],
		imagesSent: 0,
	};
}

function connected(state: SetupView, key: string, notes: ConnectionNote[] = []): ConnectOutcome {
	if (key.includes('bad')) {
		return { kind: 'rejected' };
	}
	if (key.includes('offline')) {
		return { kind: 'offline', details: 'Server: app.mivia.ai\nError: connect' };
	}
	const connection = {
		name: 'Dr. Katrin Möller',
		email: 'k.moeller@kessler-werkstoff.de',
		notes,
		remainingUploads: 1176,
	};
	Object.assign(state, { connection });
	return { kind: 'connected', connection, otherAccount: false, clipboardCleared: true };
}

const DONE: DoneView = {
	name: 'Dr. Katrin Möller',
	email: 'k.moeller@kessler-werkstoff.de',
	folder: FOLDER.display,
	folderName: FOLDER.name,
	models: ['grain', 'phases'],
	reportFolder: REPORT.display,
	existing: 0,
	waiting: [],
	problems: [],
	explorer: true,
	legacy: null,
	legacyStartup: [],
};

const INTEGRATION: IntegrationView = {
	available: true,
	autostart: 'on',
	explorer: true,
	locks: { language: false, legacySearch: true, autostart: null, explorer: null, telemetry: null },
};

type Args = Record<string, unknown>;

export function setupResponses(state: SetupView): Record<string, (args: Args) => unknown> {
	return {
		get_setup: () => state,
		get_catalogue: () => CATALOGUE,
		get_integration: () => INTEGRATION,
		set_integration: () => INTEGRATION,
		save_setup: (args) => {
			const input = args.input as SetupInput;
			const report = state.report ? { ...state.report, mode: input.reportMode } : null;
			return Object.assign(state, {
				...input,
				report,
				legacy: state.legacy && { ...state.legacy, take: input.legacyTake },
			});
		},
		pick_setup_folder: () => Object.assign(state, { folder: FOLDER, report: REPORT }),
		connect_access_key: (args) => connected(state, String(args.key ?? '')),
		import_legacy_key: () => connected(state, 'imported', ['imported']),
		complete_setup: () => {
			state.completed = true;
			return { ...DONE, models: state.models.map((model) => model.modelId) };
		},
	};
}
