import type { PreviewTarget, PreviewView } from '../../shared/ipc/bindings';
import results from './preview-results.json';

type Args = Record<string, unknown>;

function picture(edge: number): string {
	const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${edge}" height="${(edge * 3) / 4}" viewBox="0 0 400 300"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#27413a"/><stop offset="1" stop-color="#8aa79c"/></linearGradient></defs><rect width="400" height="300" fill="url(#g)"/><circle cx="130" cy="120" r="70" fill="none" stroke="#fff" stroke-width="3"/><circle cx="260" cy="190" r="50" fill="none" stroke="#fff" stroke-width="3"/><text x="200" y="285" fill="#fff" font-size="18" text-anchor="middle">${edge}</text></svg>`;
	return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

const READY: PreviewView = { kind: 'ready', width: 4000, height: 3000, display: picture(2048), view: picture(4096) };

function preview(target: PreviewTarget, scenario: string): PreviewView {
	if (scenario === 'preview-offline') {
		return { kind: 'unavailable', reason: 'offline' };
	}
	if (target.kind === 'result' && target.filename === 'overlay-b.png') {
		return { kind: 'unavailable', reason: 'notPublished' };
	}
	return READY;
}

export function previewResponses(scenario: string): Record<string, (args: Args) => unknown> {
	return {
		get_preview: (args) => preview(args.target as PreviewTarget, scenario),
		get_preview_results: (args) => ({
			resultsJson: String(args.computationId).endsWith('a') ? JSON.stringify(results) : null,
			offline: false,
		}),
		close_preview: () => null,
		note_preview: () => null,
	};
}
