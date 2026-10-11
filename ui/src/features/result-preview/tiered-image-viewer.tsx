import { ActionIcon } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconArrowsMaximize, IconMinus, IconPlus } from '@tabler/icons-react';
import { type PointerEvent, type RefObject, useEffect, useEffectEvent, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import classes from './tiered-image-viewer.module.css';
import {
	clampOffset,
	DISPLAY_EDGE,
	fitSize,
	maxZoom,
	needsView,
	type Size,
	stepZoom,
	VIEW_EDGE,
	zoomAbout,
} from './viewport';

type ViewerProps = {
	display: string;
	view: string | null;
	width: number;
	height: number;
	caption: string;
	onMaxZoom: (tier: number) => void;
};

type Placement = { zoom: number; left: number; top: number };

const NO_SIZE: Size = { width: 0, height: 0 };
const WHEEL_SPEED = 0.002;

function useElementSize(ref: RefObject<HTMLElement | null>): Size {
	const [size, setSize] = useState<Size>(NO_SIZE);
	useEffect(() => {
		const element = ref.current;
		if (!element) {
			return undefined;
		}
		const observer = new ResizeObserver(([entry]) => {
			if (entry) {
				setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
			}
		});
		observer.observe(element);
		return () => observer.disconnect();
	}, [ref]);
	return size;
}

function useTier(display: string, view: string | null, wanted: boolean): string {
	const [ready, setReady] = useState(false);
	useEffect(() => {
		if (!wanted || !view || ready) {
			return undefined;
		}
		let live = true;
		const picture = new Image();
		picture.src = view;
		picture.decode().then(
			() => live && setReady(true),
			() => undefined,
		);
		return () => {
			live = false;
		};
	}, [wanted, view, ready]);
	return ready && view ? view : display;
}

function usePlacement(box: Size, fit: Size, max: number, onMax: () => void) {
	const [stored, setStored] = useState<Placement>({ zoom: 1, left: 0, top: 0 });
	const zoom = Math.min(stored.zoom, max);
	const placed: Placement = {
		zoom,
		left: clampOffset(stored.left, fit.width * zoom, box.width),
		top: clampOffset(stored.top, fit.height * zoom, box.height),
	};
	const zoomTo = (next: number, x = box.width / 2, y = box.height / 2) => {
		const target = Math.min(max, Math.max(1, next));
		if (max > 1 && target >= max) {
			onMax();
		}
		setStored({
			zoom: target,
			left: zoomAbout(x, placed.left, zoom, target),
			top: zoomAbout(y, placed.top, zoom, target),
		});
	};
	const panBy = (dx: number, dy: number) => setStored({ ...placed, left: placed.left + dx, top: placed.top + dy });
	return { placed, zoomTo, panBy };
}

function useWheelZoom(ref: RefObject<HTMLElement | null>, zoomBy: (factor: number, x: number, y: number) => void) {
	const latest = useEffectEvent(zoomBy);
	useEffect(() => {
		const element = ref.current;
		const onWheel = (event: WheelEvent) => {
			event.preventDefault();
			const rect = element?.getBoundingClientRect();
			latest(
				Math.exp(-event.deltaY * WHEEL_SPEED),
				event.clientX - (rect?.left ?? 0),
				event.clientY - (rect?.top ?? 0),
			);
		};
		element?.addEventListener('wheel', onWheel, { passive: false });
		return () => element?.removeEventListener('wheel', onWheel);
	}, [ref]);
}

function Controls({ zoom, max, zoomTo }: { zoom: number; max: number; zoomTo: (next: number) => void }) {
	const { t } = useTranslation();
	return (
		<div className={classes.controls}>
			<ActionIcon
				aria-label={t('desktop.preview.zoomOut')}
				disabled={zoom <= 1}
				onClick={() => zoomTo(stepZoom(zoom, -1, max))}
				variant="default"
			>
				<IconMinus size={16} stroke={ICON_STROKE} />
			</ActionIcon>
			<ActionIcon
				aria-label={t('desktop.preview.zoomIn')}
				disabled={zoom >= max}
				onClick={() => zoomTo(stepZoom(zoom, 1, max))}
				variant="default"
			>
				<IconPlus size={16} stroke={ICON_STROKE} />
			</ActionIcon>
			<ActionIcon
				aria-label={t('desktop.preview.zoomFit')}
				disabled={zoom <= 1}
				onClick={() => zoomTo(1)}
				variant="default"
			>
				<IconArrowsMaximize size={16} stroke={ICON_STROKE} />
			</ActionIcon>
		</div>
	);
}

function useDrag(panBy: (dx: number, dy: number) => void) {
	const dragging = useRef(false);
	return {
		onPointerDown: (event: PointerEvent<HTMLElement>) => {
			if (event.target instanceof Element && event.target.closest('button')) {
				return;
			}
			dragging.current = true;
			event.currentTarget.setPointerCapture(event.pointerId);
		},
		onPointerMove: (event: PointerEvent) => dragging.current && panBy(event.movementX, event.movementY),
		onPointerUp: () => {
			dragging.current = false;
		},
	};
}

export function TieredImageViewer({ display, view, width, height, caption, onMaxZoom }: ViewerProps) {
	const ref = useRef<HTMLDivElement>(null);
	const box = useElementSize(ref);
	const image: Size = { width, height };
	const ratio = window.devicePixelRatio;
	const fit = box.width > 0 ? fitSize(box, image) : NO_SIZE;
	const max = fit.width > 0 ? maxZoom(fit.width, ratio, image, view !== null) : 1;
	const { placed, zoomTo, panBy } = usePlacement(box, fit, max, () => onMaxZoom(view ? VIEW_EDGE : DISPLAY_EDGE));
	const src = useTier(display, view, needsView(fit.width, placed.zoom, ratio, image));
	useWheelZoom(ref, (factor, x, y) => zoomTo(placed.zoom * factor, x, y));
	const drag = useDrag(panBy);
	return (
		<div className={classes.viewer} data-zoomed={placed.zoom > 1} ref={ref} {...drag}>
			{fit.width > 0 && (
				<img
					alt={caption}
					className={classes.picture}
					decoding="async"
					draggable={false}
					src={src}
					style={{
						width: fit.width,
						height: fit.height,
						transform: `translate(${placed.left}px, ${placed.top}px) scale(${placed.zoom})`,
					}}
				/>
			)}
			<Controls max={max} zoom={placed.zoom} zoomTo={zoomTo} />
		</div>
	);
}
