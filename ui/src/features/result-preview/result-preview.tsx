import { Group, Stack, Text } from '@mantine/core';
import { Component, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { BarChart } from './bar-chart';
import {
	type BarChartResult,
	type HistogramResult,
	type ImageResult,
	keyed,
	ofType,
	type Result,
	xText,
} from './results';
import { DataTable, Message, MultiColumnTable } from './tables';
import { useLabel } from './use-label';

type ResultPreviewProps = { results: Result[]; renderImage: (result: ImageResult) => ReactNode };

function Unavailable() {
	const { t } = useTranslation();
	return (
		<Text c="dimmed" fs="italic" size="sm">
			{t('desktop.viz.resultUnavailable')}
		</Text>
	);
}

class Isolated extends Component<{ children: ReactNode }, { failed: boolean }> {
	override state = { failed: false };

	static getDerivedStateFromError() {
		return { failed: true };
	}

	override render() {
		return this.state.failed ? <Unavailable /> : this.props.children;
	}
}

function Bars({ result }: { result: BarChartResult }) {
	const label = useLabel();
	const key = result.keys.includes('fraction') ? 'fraction' : (result.keys[0] ?? 'fraction');
	const bars = result.data.map((item) => ({
		label: label(item.label),
		value: Number(item[key] ?? 0),
		color: item.color,
	}));
	const yLabel = label(result.yLabels?.[result.keys.indexOf(key)]);
	return <BarChart bars={bars} title={yLabel} xLabel={label(result.xLabel)} yLabel={yLabel} />;
}

function Histogram({ result }: { result: HistogramResult }) {
	const label = useLabel();
	const unit = result.unit ? ` [${label(result.unit)}]` : '';
	const bars = result.data.map((item) => ({ label: xText(item.x), value: item.y, color: item.color }));
	return (
		<BarChart
			bars={bars}
			title={label(result.label)}
			xLabel={`${label(result.xLabel)}${unit}`}
			yLabel={label(result.yLabel)}
		/>
	);
}

function isolated<T extends Result>(items: T[], render: (item: T) => ReactNode) {
	return keyed(items).map(({ id, item }) => <Isolated key={id}>{render(item)}</Isolated>);
}

export function ResultPreview({ results, renderImage }: ResultPreviewProps) {
	return (
		<Stack gap="sm">
			{isolated(ofType(results, 'message'), (result) => (
				<Message result={result} />
			))}
			<Group align="flex-start" gap="md">
				{isolated(ofType(results, 'image'), renderImage)}
				{isolated(ofType(results, 'bar-chart'), (result) => (
					<Bars result={result} />
				))}
				{isolated(ofType(results, 'data-table'), (result) => (
					<DataTable result={result} />
				))}
				{isolated(ofType(results, 'multi-column-table'), (result) => (
					<MultiColumnTable result={result} />
				))}
				{isolated(ofType(results, 'histogram'), (result) => (
					<Histogram result={result} />
				))}
			</Group>
		</Stack>
	);
}
