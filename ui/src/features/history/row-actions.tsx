import { ActionIcon, Group, Menu } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconDots } from '@tabler/icons-react';
import { useMutation } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { HistoryRow } from '../../shared/ipc/bindings';
import { ShowInFolderIcon } from '../../shared/ui/icons';
import { useExport } from './use-export';

export function RowActions({ row }: { row: HistoryRow }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const reveal = useMutation({ mutationFn: (partId: string) => commands.openTarget({ kind: 'revealReport', partId }) });
	const exporter = useExport();
	const { summary, savedPart } = row;
	return (
		<Group gap={4} justify="flex-end" wrap="nowrap">
			{savedPart && (
				<ActionIcon
					aria-label={t('desktop.series.showReports')}
					onClick={() => reveal.mutate(savedPart)}
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
					<Menu.Item onClick={() => navigate({ to: '/series/$seriesId', params: { seriesId: summary.id } })}>
						{t('desktop.series.open')}
					</Menu.Item>
					<Menu.Item onClick={() => exporter.mutate({ filter: null, series: summary.id })}>
						{t('desktop.history.exportSeries')}
					</Menu.Item>
				</Menu.Dropdown>
			</Menu>
		</Group>
	);
}
