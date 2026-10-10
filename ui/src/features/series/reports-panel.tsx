import { Button, Divider, Group, Stack, Text, ThemeIcon } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation } from '@tanstack/react-query';
import { Fragment } from 'react';
import { useTranslation } from 'react-i18next';

import { analysisName } from '../../entities/series/status';
import { commands } from '../../shared/ipc';
import type { AnalysisView, Language, OpenTarget, ReportView } from '../../shared/ipc/bindings';
import { OpenIcon, ReportIcon, ShowInFolderIcon } from '../../shared/ui/icons';
import { Panel } from '../../shared/ui/panel';
import { StatusBadge } from '../../shared/ui/status-badge';
import { type ReportRow, reportRows, reportStatus } from './report-rows';

function RowActions({ row }: { row: ReportRow }) {
	const { t } = useTranslation();
	const open = useMutation({ mutationFn: (target: OpenTarget) => commands.openTarget(target) });
	const retry = useMutation({ mutationFn: (id: string) => commands.tryAgainReportPart(id) });
	const partId = row.open;
	return (
		<Group gap="xs" justify="flex-end" w={250} wrap="nowrap">
			{partId && (
				<>
					<Button
						leftSection={<ShowInFolderIcon size={14} stroke={ICON_STROKE} />}
						onClick={() => open.mutate({ kind: 'revealReport', partId })}
						size="xs"
						variant="subtle"
					>
						{t('desktop.series.showInFolder')}
					</Button>
					<Button
						leftSection={<OpenIcon size={14} stroke={ICON_STROKE} />}
						onClick={() => open.mutate({ kind: 'report', partId })}
						size="xs"
						variant="default"
					>
						{t('desktop.series.open')}
					</Button>
				</>
			)}
			{row.retry && !partId && (
				<Button onClick={() => row.retry && retry.mutate(row.retry)} size="xs" variant="default">
					{t('desktop.attention.tryAgain')}
				</Button>
			)}
		</Group>
	);
}

function RowText({ row, name }: { row: ReportRow; name: string }) {
	const { t } = useTranslation();
	const title = row.partCount > 1 ? t('desktop.series.part', { name, part: row.partNo, parts: row.partCount }) : name;
	return (
		<Stack flex={1} gap={0} miw={0}>
			<Text fw={500} size="sm" truncate>
				{title}
			</Text>
			{row.to > 0 && (
				<Text c="stone.7" size="xs">
					{t('desktop.series.range', { from: row.from, to: row.to, formats: row.formats.join(' + ') })}
				</Text>
			)}
		</Stack>
	);
}

type ReportsPanelProps = { reports: ReportView[]; analyses: AnalysisView[]; language: Language };

export function ReportsPanel({ reports, analyses, language }: ReportsPanelProps) {
	const { t } = useTranslation();
	const rows = reportRows(reports);
	const split = rows.some((row) => row.partCount > 1);
	return (
		<Panel
			extra={
				split && (
					<Text c="stone.7" size="xs">
						{t('desktop.series.splitHint')}
					</Text>
				)
			}
			title={t('desktop.series.reports')}
		>
			{rows.length === 0 && (
				<Text c="stone.7" size="sm">
					{t('desktop.series.noReportsYet')}
				</Text>
			)}
			<Stack gap={0}>
				{rows.map((row, index) => (
					<Fragment key={row.key}>
						{index > 0 && <Divider />}
						<Group gap="md" py="xs" wrap="nowrap">
							<ThemeIcon color="stone" size={36} variant="light">
								<ReportIcon size={20} stroke={ICON_STROKE} />
							</ThemeIcon>
							<RowText
								name={analysisName(
									analyses.find((analysis) => analysis.key === row.report.analysisKey),
									language,
								)}
								row={row}
							/>
							<StatusBadge status={reportStatus(row, t)} />
							<RowActions row={row} />
						</Group>
					</Fragment>
				))}
			</Stack>
		</Panel>
	);
}
