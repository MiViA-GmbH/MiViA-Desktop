import { Text } from '@mantine/core';

type Bar = { label: string; value: number; color: string };

type BarChartProps = { title: string; bars: Bar[]; xLabel: string; yLabel: string };

const WIDTH = 560;
const HEIGHT = 300;
const LEFT = 56;
const TOP = 10;
const BOTTOM = 84;
const MAX_LABELS = 20;

function ticks(top: number): number[] {
	return [0, top / 2, top];
}

function shorten(value: number): string {
	return String(Math.round(value * 10) / 10);
}

export function BarChart({ title, bars, xLabel, yLabel }: BarChartProps) {
	const top = Math.max(...bars.map((bar) => bar.value), 0) || 1;
	const inner = { width: WIDTH - LEFT - 12, height: HEIGHT - TOP - BOTTOM };
	const slot = inner.width / Math.max(bars.length, 1);
	const ids = bars.map((bar, position) => `${position}:${bar.label}`);
	const every = Math.ceil(bars.length / MAX_LABELS);
	const y = (value: number) => TOP + inner.height - (Math.max(value, 0) / top) * inner.height;
	return (
		<figure style={{ margin: 0, maxWidth: WIDTH }}>
			<Text fw={600} size="sm">
				{title}
			</Text>
			<svg aria-label={title} role="img" style={{ width: '100%', height: 'auto' }} viewBox={`0 0 ${WIDTH} ${HEIGHT}`}>
				<title>{title}</title>
				{ticks(top).map((value) => (
					<g key={value}>
						<line stroke="currentColor" strokeOpacity={0.15} x1={LEFT} x2={WIDTH - 12} y1={y(value)} y2={y(value)} />
						<text fill="currentColor" fontSize={11} textAnchor="end" x={LEFT - 6} y={y(value) + 4}>
							{shorten(value)}
						</text>
					</g>
				))}
				{bars.map((bar, index) => (
					<g key={ids[index]}>
						<rect
							fill={bar.color}
							height={TOP + inner.height - y(bar.value)}
							width={slot * 0.8}
							x={LEFT + slot * index + slot * 0.1}
							y={y(bar.value)}
						>
							<title>{`${bar.label}: ${shorten(bar.value)}`}</title>
						</rect>
						{index % every === 0 && (
							<text
								fill="currentColor"
								fontSize={11}
								textAnchor="end"
								transform={`rotate(-45 ${LEFT + slot * (index + 0.5)} ${TOP + inner.height + 10})`}
								x={LEFT + slot * (index + 0.5)}
								y={TOP + inner.height + 14}
							>
								{bar.label}
							</text>
						)}
					</g>
				))}
				<text fill="currentColor" fontSize={12} textAnchor="middle" x={LEFT + inner.width / 2} y={HEIGHT - 4}>
					{xLabel}
				</text>
				<text
					fill="currentColor"
					fontSize={12}
					textAnchor="middle"
					transform={`rotate(-90 12 ${TOP + inner.height / 2})`}
					x={12}
					y={TOP + inner.height / 2}
				>
					{yLabel}
				</text>
			</svg>
		</figure>
	);
}
