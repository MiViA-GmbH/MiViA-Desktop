import { ActionIcon, Group, Menu } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconDots } from '@tabler/icons-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useReporting } from '../../entities/support/hooks';
import { requestReport } from '../../entities/support/queries';
import { commands } from '../../shared/ipc';
import type { HistoryRow, OpenTarget } from '../../shared/ipc/bindings';
import { ShowInFolderIcon } from '../../shared/ui/icons';
import { CustomReportDialog } from './custom-report-dialog';
import { DeleteDialog } from './delete-dialog';
import { useExport } from './use-export';

export function RowActions({ row }: { row: HistoryRow }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const open = useMutation({ mutationFn: (target: OpenTarget) => commands.openTarget(target) });
	const exporter = useExport();
	const client = useQueryClient();
	const reporting = useReporting();
	const [remove, setRemove] = useState(false);
	const [csv, setCsv] = useState(false);
	const { summary, savedPart } = row;
	const failed = summary.counts.failed > 0 || summary.counts.reportsProblem > 0;
	return (
		<Group gap={4} justify="flex-end" wrap="nowrap">
			{savedPart && (
				<ActionIcon
					aria-label={t('desktop.series.showReports')}
					color="stone"
					onClick={() => open.mutate({ kind: 'revealReport', partId: savedPart })}
					title={t('desktop.series.showReports')}
					variant="subtle"
				>
					<ShowInFolderIcon size={18} stroke={ICON_STROKE} />
				</ActionIcon>
			)}
			<Menu position="bottom-end" withinPortal>
				<Menu.Target>
					<ActionIcon aria-label={t('desktop.history.menu')} color="stone" variant="subtle">
						<IconDots size={18} stroke={ICON_STROKE} />
					</ActionIcon>
				</Menu.Target>
				<Menu.Dropdown>
					{savedPart && (
						<Menu.Item onClick={() => open.mutate({ kind: 'report', partId: savedPart })}>
							{t('desktop.history.openReport')}
						</Menu.Item>
					)}
					<Menu.Item onClick={() => navigate({ to: '/series/$seriesId', params: { seriesId: summary.id } })}>
						{t('desktop.history.showSeries')}
					</Menu.Item>
					<Menu.Item onClick={() => setCsv(true)}>{t('desktop.history.saveResults')}</Menu.Item>
					<Menu.Item onClick={() => exporter.mutate({ filter: null, series: summary.id })}>
						{t('desktop.history.exportSeries')}
					</Menu.Item>
					{failed && reporting && (
						<Menu.Item onClick={() => requestReport(client, { trigger: 'error', signature: null })}>
							{t('desktop.support.reportAction')}
						</Menu.Item>
					)}
					<Menu.Item onClick={() => setRemove(true)}>{t('desktop.delete.seriesAction')}</Menu.Item>
				</Menu.Dropdown>
			</Menu>
			{csv && <CustomReportDialog csvOnly onClose={() => setCsv(false)} series={[summary.id]} />}
			{remove && (
				<DeleteDialog
					onClose={() => setRemove(false)}
					subject={{ series: summary.id, name: summary.label, images: summary.counts.files }}
				/>
			)}
		</Group>
	);
}
