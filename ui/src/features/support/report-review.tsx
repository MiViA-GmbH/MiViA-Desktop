import { Code, Paper, ScrollArea, Select, Stack, Text } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { ProblemReportPreview } from '../../shared/ipc/bindings';
import { formatFullDateTime, formatSize } from '../../shared/lib/format';

function FileViewer({ preview }: { preview: ProblemReportPreview }) {
	const { t } = useTranslation();
	const [name, setName] = useState<string | null>(null);
	const text = useQuery({
		queryKey: ['reportFile', preview.reportId, name],
		queryFn: () => commands.readProblemReportFile(preview.reportId, name ?? ''),
		enabled: name !== null,
	}).data;
	const files = preview.entries.map((entry) => ({
		value: entry.name,
		label: `${entry.name} (${formatSize(entry.bytes)})`,
	}));
	return (
		<Stack gap="xs">
			<Select data={files} label={t('desktop.support.showFile')} onChange={setName} size="xs" value={name} />
			{name !== null && text !== undefined && (
				<ScrollArea.Autosize
					mah="clamp(40px, calc(100dvh - 410px), 220px)"
					type="auto"
					viewportProps={{ tabIndex: 0, 'aria-label': name }}
				>
					<Code block style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>
						{text}
					</Code>
				</ScrollArea.Autosize>
			)}
		</Stack>
	);
}

function Summary({ preview }: { preview: ProblemReportPreview }) {
	const { t } = useTranslation();
	const size = formatSize(preview.totalBytes);
	return (
		<Stack gap={4}>
			<Text size="sm">{t('desktop.support.files', { count: preview.entries.length, size })}</Text>
			{preview.truncated && <Text size="sm">{t('desktop.support.truncated')}</Text>}
			<Text size="sm">
				{t('desktop.support.period', { from: formatFullDateTime(preview.from), to: formatFullDateTime(preview.to) })}
			</Text>
			<Text c="stone.7" size="xs">
				{t('desktop.support.ids', { installId: preview.installId, sessions: preview.sessionIds.length })}
			</Text>
			{preview.firstErrors.length > 0 && (
				<Text size="sm">{t('desktop.support.firstErrors', { errors: preview.firstErrors.join(', ') })}</Text>
			)}
		</Stack>
	);
}

export function ReportReview({ preview }: { preview: ProblemReportPreview }) {
	return (
		<Paper bg="stone.0" p="sm" withBorder>
			<Stack gap="sm">
				<Summary preview={preview} />
				<FileViewer preview={preview} />
			</Stack>
		</Paper>
	);
}
