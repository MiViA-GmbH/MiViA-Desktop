export type MessageResult = {
	type: 'message';
	label?: string;
	data?: { title: string; content: string; status: 'info' | 'warning' | 'error' };
};
export type ImageResult = { type: 'image'; label?: string; filename: string };
export type BarChartResult = {
	type: 'bar-chart';
	keys: string[];
	data: { label: string; color: string; [key: string]: string | number }[];
	xLabel?: string;
	yLabels?: string[];
};
export type DataTableResult = {
	type: 'data-table';
	data: { label: string; labelTooltip?: string; value: number | string; unit?: string; group?: string }[];
};
export type MultiColumnTableResult = {
	type: 'multi-column-table';
	label?: string;
	headers?: string[];
	units?: (string | null)[];
	unit?: (string | null)[];
	data: (string | number | null)[][];
};
export type HistogramResult = {
	type: 'histogram';
	label?: string;
	xLabel?: string;
	yLabel?: string;
	unit?: string;
	data: { x: number | string | [number, number]; y: number; color: string }[];
};
export type Result =
	| MessageResult
	| ImageResult
	| BarChartResult
	| DataTableResult
	| MultiColumnTableResult
	| HistogramResult;

export function parseResults(json: string | null): Result[] {
	if (json === null) {
		return [];
	}
	try {
		const parsed: unknown = JSON.parse(json);
		if (!Array.isArray(parsed)) {
			return [];
		}
		return parsed.filter((item) => typeof item === 'object' && item !== null);
	} catch {
		return [];
	}
}

export function ofType<T extends Result['type']>(results: Result[], type: T): Extract<Result, { type: T }>[] {
	return results.filter((result): result is Extract<Result, { type: T }> => result.type === type);
}

export function roundValue(value: number | string): string {
	return typeof value === 'string' ? value : String(Math.round(value * 100) / 100);
}

export function xText(x: number | string | [number, number]): string {
	if (Array.isArray(x)) {
		return `${roundValue(x[0])}-${roundValue(x[1])}`;
	}
	return roundValue(x);
}

export function groupItems<T extends { group?: string }>(items: T[]): { group: string | undefined; items: T[] }[] {
	const groups = new Map<string | undefined, T[]>();
	for (const item of items) {
		groups.set(item.group, [...(groups.get(item.group) ?? []), item]);
	}
	return [...groups].map(([group, members]) => ({ group, items: members }));
}

export function keyed<T>(items: T[]): { id: string; item: T }[] {
	return items.map((item, position) => ({ id: String(position), item }));
}
