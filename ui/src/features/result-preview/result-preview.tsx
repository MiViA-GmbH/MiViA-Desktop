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

type ResultError = (type: Result['type'], error: Error) => void;

type ResultPreviewProps = {
	results: Result[];
	renderImage: (result: ImageResult) => ReactNode;
	onResultError: ResultError;
};

function Unavailable() {
	const { t } = useTranslation();
	return (
		<Text c="stone.7" size="sm">
			{t('desktop.viz.resultUnavailable')}
		</Text>
	);
}

class Isolated extends Component<{ children: ReactNode; onError: (error: Error) => void }, { failed: boolean }> {
	override state = { failed: false };

	static getDerivedStateFromError() {
		return { failed: true };
	}

	override componentDidCatch(error: Error) {
		this.props.onError(error);
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

function isolated<T extends Result>(onError: ResultError, items: T[], render: (item: T) => ReactNode) {
	return keyed(items).map(({ id, item }) => (
		<Isolated key={id} onError={(error) => onError(item.type, error)}>
			{render(item)}
		</Isolated>
	));
}

export function ResultPreview({ results, renderImage, onResultError }: ResultPreviewProps) {
	return (
		<Stack gap="sm">
			{isolated(onResultError, ofType(results, 'message'), (result) => (
				<Message result={result} />
			))}
			<Group align="flex-start" gap="md">
				{isolated(onResultError, ofType(results, 'image'), renderImage)}
				{isolated(onResultError, ofType(results, 'bar-chart'), (result) => (
					<Bars result={result} />
				))}
				{isolated(onResultError, ofType(results, 'data-table'), (result) => (
					<DataTable result={result} />
				))}
				{isolated(onResultError, ofType(results, 'multi-column-table'), (result) => (
					<MultiColumnTable result={result} />
				))}
				{isolated(onResultError, ofType(results, 'histogram'), (result) => (
					<Histogram result={result} />
				))}
			</Group>
		</Stack>
	);
}
