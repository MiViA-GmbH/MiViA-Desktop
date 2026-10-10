import { Button } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { FolderView } from '../../shared/ipc/bindings';
import { ReportIcon, ResumeIcon } from '../../shared/ui/icons';

export function FinishSeriesButton({ seriesId, variant }: { seriesId: string; variant: 'subtle' | 'default' }) {
	const { t } = useTranslation();
	const finish = useMutation({ mutationFn: () => commands.finishSeries(seriesId) });
	return (
		<Button loading={finish.isPending} onClick={() => finish.mutate()} size="xs" variant={variant}>
			{t('desktop.folder.finishSeries')}
		</Button>
	);
}

export function ResumeButton({ folderId }: { folderId: string }) {
	const { t } = useTranslation();
	const resume = useMutation({ mutationFn: () => commands.resumeFolder(folderId) });
	return (
		<Button
			leftSection={<ResumeIcon size={14} stroke={ICON_STROKE} />}
			loading={resume.isPending}
			onClick={() => resume.mutate()}
			size="xs"
			variant="subtle"
		>
			{t('desktop.folder.resume')}
		</Button>
	);
}

export function LatestReportButton({ folder }: { folder: FolderView }) {
	const { t } = useTranslation();
	const open = useMutation({
		mutationFn: (partId: string) => commands.openTarget({ kind: 'report', partId }),
	});
	const partId = folder.latestReport;
	if (!partId) {
		return null;
	}
	return (
		<Button
			leftSection={<ReportIcon size={14} stroke={ICON_STROKE} />}
			onClick={() => open.mutate(partId)}
			size="xs"
			variant="default"
		>
			{t('desktop.folder.latestReport')}
		</Button>
	);
}
