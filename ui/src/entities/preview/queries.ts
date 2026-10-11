import { queryOptions } from '@tanstack/react-query';

import { assetUrl, commands } from '../../shared/ipc';
import type { PreviewReason, PreviewTarget } from '../../shared/ipc/bindings';

type Picture =
	| { kind: 'ready'; width: number; height: number; display: string; view: string | null }
	| { kind: 'unavailable'; reason: PreviewReason };

async function load(target: PreviewTarget): Promise<Picture> {
	const preview = await commands.getPreview(target);
	if (preview.kind === 'unavailable') {
		return preview;
	}
	return {
		...preview,
		display: assetUrl(preview.display),
		view: preview.view === null ? null : assetUrl(preview.view),
	};
}

export const pictureQuery = (target: PreviewTarget) =>
	queryOptions({ queryKey: ['preview', 'picture', target], queryFn: () => load(target), gcTime: 0, retry: false });

export const previewResultsQuery = (computationId: string) =>
	queryOptions({
		queryKey: ['preview', 'results', computationId],
		queryFn: () => commands.getPreviewResults(computationId),
		gcTime: 0,
		retry: false,
	});
