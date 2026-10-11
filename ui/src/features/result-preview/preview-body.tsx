import { Button, Group, Stack, Text, Title, UnstyledButton } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { pictureQuery, previewResultsQuery } from '../../entities/preview/queries';
import { analysisName } from '../../entities/series/status';
import { commands } from '../../shared/ipc';
import type { AnalysisView, BatchItem, ComputationItem, Language, PreviewTarget } from '../../shared/ipc/bindings';
import { Notice } from '../../shared/ui/notice';
import { PicturePane } from './picture-pane';
import { ResultPreview } from './result-preview';
import { type ImageResult, parseResults } from './results';
import { useLabel } from './use-label';

type BodyProps = { item: BatchItem; analyses: AnalysisView[]; language: Language };
type Pick = { active: PreviewTarget; onPick: (target: PreviewTarget) => void };

const THUMBNAIL = 350;

function same(a: PreviewTarget, b: PreviewTarget): boolean {
	return JSON.stringify(a) === JSON.stringify(b);
}

function ImageChoice({ computationId, result, active, onPick }: Pick & { computationId: string; result: ImageResult }) {
	const { t } = useTranslation();
	const label = useLabel();
	const target: PreviewTarget = { kind: 'result', computationId, filename: result.filename };
	const { data } = useQuery(pictureQuery(target));
	const name = label(result.label) || result.filename;
	return (
		<UnstyledButton aria-pressed={same(active, target)} onClick={() => onPick(target)} w={THUMBNAIL}>
			{data?.kind === 'ready' ? (
				<img alt={name} decoding="async" src={data.display} style={{ width: '100%', display: 'block' }} />
			) : (
				<Text c="stone.7" size="sm">
					{data ? t('desktop.preview.unavailable') : t('desktop.preview.loading')}
				</Text>
			)}
			<Text size="sm">{name}</Text>
		</UnstyledButton>
	);
}

function Results({ computation, pick }: { computation: ComputationItem; pick: Pick }) {
	const { t } = useTranslation();
	const { data, isPending, refetch } = useQuery(previewResultsQuery(computation.id));
	const results = useMemo(() => parseResults(data?.resultsJson ?? null), [data]);
	if (isPending) {
		return null;
	}
	if (data?.offline) {
		return (
			<Notice title={t('desktop.preview.resultsOffline')} tone="warn">
				<Button onClick={() => refetch()} size="compact-sm" variant="default">
					{t('desktop.preview.retry')}
				</Button>
			</Notice>
		);
	}
	return (
		<ResultPreview
			renderImage={(result) => (
				<ImageChoice active={pick.active} computationId={computation.id} onPick={pick.onPick} result={result} />
			)}
			results={results}
		/>
	);
}

function useNotes(computation: ComputationItem | undefined) {
	const [started] = useState(() => performance.now());
	const opened = useRef(false);
	const zoomed = useRef(false);
	const id = computation?.id;
	const onReady = useCallback(() => {
		if (id && !opened.current) {
			opened.current = true;
			commands.notePreview({
				kind: 'opened',
				computationId: id,
				durationMs: Math.round(performance.now() - started),
			});
		}
	}, [id, started]);
	const onMaxZoom = useCallback(
		(tier: number, longEdge: number) => {
			if (id && !zoomed.current) {
				zoomed.current = true;
				commands.notePreview({ kind: 'zoomMaxTier', computationId: id, tier, longEdge });
			}
		},
		[id],
	);
	return { onReady, onMaxZoom };
}

export function PreviewBody({ item, analyses, language }: BodyProps) {
	const { t } = useTranslation();
	const [active, setActive] = useState<PreviewTarget>({ kind: 'source', sourceFileId: item.id });
	const done = item.computations.filter((computation) => computation.state === 'done');
	const { onReady, onMaxZoom } = useNotes(done[0] ?? item.computations[0]);
	useEffect(() => () => void commands.closePreview(), []);
	const source: PreviewTarget = { kind: 'source', sourceFileId: item.id };
	return (
		<Stack gap="lg">
			{!same(active, source) && (
				<Group>
					<Button onClick={() => setActive(source)} size="compact-sm" variant="default">
						{t('desktop.preview.source')}
					</Button>
				</Group>
			)}
			<PicturePane
				caption={item.label}
				key={JSON.stringify(active)}
				onMaxZoom={onMaxZoom}
				onReady={onReady}
				target={active}
			/>
			{done.map((computation) => (
				<Stack gap="xs" key={computation.id}>
					<Title order={4}>
						{analysisName(
							analyses.find((analysis) => analysis.key === computation.modelChoiceKey),
							language,
						)}
					</Title>
					<Results computation={computation} pick={{ active, onPick: setActive }} />
				</Stack>
			))}
		</Stack>
	);
}
