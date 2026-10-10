export type FieldType = 'number' | 'flag' | 'id' | 'version' | 'choice';

export type ListedField = { name: string; type: FieldType; values?: string[] };

export type ListedEvent = {
	name: string;
	category: 'diagnostics' | 'usage';
	priority: 'core' | 'interaction';
	purpose: string;
	fields: ListedField[];
};

export type FieldList = { schema: number; retentionDays: number; events: ListedEvent[] };

export async function loadFieldList(language: string): Promise<FieldList> {
	const loaded = language.startsWith('de')
		? await import('./field-list.de.json')
		: await import('./field-list.en.json');
	return loaded.default as FieldList;
}
