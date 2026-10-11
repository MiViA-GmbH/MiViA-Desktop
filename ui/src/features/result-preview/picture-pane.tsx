import { Button, Group, Loader, Stack, Text } from '@mantine/core';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { pictureQuery } from '../../entities/preview/queries';
import { commands } from '../../shared/ipc';
import type { OpenTarget, PreviewReason, PreviewTarget } from '../../shared/ipc/bindings';
import { OpenIcon } from '../../shared/ui/icons';
import { Notice } from '../../shared/ui/notice';
import { TieredImageViewer } from './tiered-image-viewer';

type PaneProps = {
	target: PreviewTarget;
	caption: string;
	onReady: () => void;
	onMaxZoom: (tier: number, longEdge: number) => void;
};

const REASON_TEXT = {
	missing: 'desktop.preview.reason.missing',
	tooLarge: 'desktop.preview.reason.tooLarge',
	unsupported: 'desktop.preview.reason.unsupported',
	corrupt: 'desktop.preview.reason.corrupt',
	offline: 'desktop.preview.reason.offline',
	notPublished: 'desktop.preview.reason.notPublished',
	cancelled: 'desktop.preview.reason.cancelled',
} as const satisfies Record<PreviewReason, string>;

const WARNING_REASONS = new Set<PreviewReason>(['offline', 'notPublished']);

function originalOf(target: PreviewTarget): OpenTarget {
	return target.kind === 'source'
		? { kind: 'original', sourceFileId: target.sourceFileId }
		: { kind: 'resultImage', computationId: target.computationId, filename: target.filename };
}

function OpenOriginal({ target }: { target: PreviewTarget }) {
	const { t } = useTranslation();
	const open = useMutation({ mutationFn: () => commands.openTarget(originalOf(target)) });
	return (
		<Button
			leftSection={<OpenIcon size={16} />}
			loading={open.isPending}
			onClick={() => open.mutate()}
			size="compact-sm"
			variant="subtle"
		>
			{t('desktop.preview.openOriginal')}
		</Button>
	);
}

function NotAvailable({ reason, target }: { reason: PreviewReason; target: PreviewTarget }) {
	const { t } = useTranslation();
	return (
		<Notice title={t('desktop.preview.unavailable')} tone={WARNING_REASONS.has(reason) ? 'info' : 'warn'}>
			<Stack align="flex-start" gap="xs">
				<Text size="sm">{t(REASON_TEXT[reason])}</Text>
				{reason !== 'notPublished' && <OpenOriginal target={target} />}
			</Stack>
		</Notice>
	);
}

export function PicturePane({ target, caption, onReady, onMaxZoom }: PaneProps) {
	const { t } = useTranslation();
	const { data, isPending, isError } = useQuery(pictureQuery(target));
	const ready = data?.kind === 'ready';
	useEffect(() => {
		if (ready) {
			onReady();
		}
	}, [ready, onReady]);
	if (isPending) {
		return (
			<Group gap="xs" py="xl">
				<Loader size="sm" />
				<Text c="stone.7" size="sm">
					{t('desktop.preview.loading')}
				</Text>
			</Group>
		);
	}
	if (isError || data?.kind !== 'ready') {
		return <NotAvailable reason={data?.kind === 'unavailable' ? data.reason : 'corrupt'} target={target} />;
	}
	const longEdge = Math.max(data.width, data.height);
	return (
		<Stack gap={4}>
			<TieredImageViewer
				caption={caption}
				display={data.display}
				height={data.height}
				onMaxZoom={(tier) => onMaxZoom(tier, longEdge)}
				view={data.view}
				width={data.width}
			/>
			<Group justify="flex-end">
				<OpenOriginal target={target} />
			</Group>
		</Stack>
	);
}
