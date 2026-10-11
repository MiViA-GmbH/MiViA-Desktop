export type Size = { width: number; height: number };

export const DISPLAY_EDGE = 2048;
export const VIEW_EDGE = 4096;
const UPSCALE_TOLERANCE = 1.25;
const ZOOM_STEP = 1.5;

export function fitSize(container: Size, image: Size): Size {
	const scale = Math.min(container.width / image.width, container.height / image.height);
	return { width: image.width * scale, height: image.height * scale };
}

export function tierWidth(image: Size, edge: number): number {
	const longEdge = Math.max(image.width, image.height);
	return (image.width * Math.min(edge, longEdge)) / longEdge;
}

export function needsView(fitWidth: number, zoom: number, ratio: number, image: Size): boolean {
	return fitWidth * zoom * ratio > tierWidth(image, DISPLAY_EDGE) * UPSCALE_TOLERANCE;
}

export function maxZoom(fitWidth: number, ratio: number, image: Size, hasView: boolean): number {
	const largest = tierWidth(image, hasView ? VIEW_EDGE : DISPLAY_EDGE);
	return Math.max(1, (largest * UPSCALE_TOLERANCE) / (fitWidth * ratio));
}

export function clampOffset(offset: number, content: number, container: number): number {
	if (content <= container) {
		return (container - content) / 2;
	}
	return Math.min(0, Math.max(container - content, offset));
}

export function zoomAbout(point: number, offset: number, from: number, to: number): number {
	return point - ((point - offset) * to) / from;
}

export function stepZoom(zoom: number, direction: 1 | -1, max: number): number {
	const next = direction === 1 ? zoom * ZOOM_STEP : zoom / ZOOM_STEP;
	return Math.min(max, Math.max(1, next));
}
