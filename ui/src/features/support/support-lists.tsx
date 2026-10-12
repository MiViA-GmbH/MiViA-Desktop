import { Button, Group, Stack, Table, Text } from '@mantine/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { pendingReportsQuery, SUPPORT } from '../../entities/support/queries';
import { saveReport } from '../../entities/support/toasts';
import { commands } from '../../shared/ipc';
import type { PendingReportView, PolicyRow } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';

const SOURCES = { machine: 'desktop.support.machine', user: 'desktop.support.user' } as const;

function PendingRow({ report }: { report: PendingReportView }) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const cancel = useMutation({
		mutationFn: () => commands.cancelPendingReport(report.reportId),
		onSuccess: () => client.invalidateQueries({ queryKey: SUPPORT }),
	});
	const state = report.state === 'waiting' ? t('desktop.support.waiting') : t('desktop.support.mustSave');
	return (
		<Group gap="sm" justify="space-between" wrap="nowrap">
			<Stack gap={0} miw={0}>
				<Text ff="monospace" size="sm">
					{report.reference}
				</Text>
				<Text c="stone.7" size="xs">
					{`${formatDateTime(report.createdAt)} · ${state}`}
				</Text>
			</Stack>
			<Group gap="xs" wrap="nowrap">
				<Button onClick={() => saveReport(report.reportId, client)} size="xs" variant="default">
					{t('desktop.support.saveAndEmail')}
				</Button>
				<Button loading={cancel.isPending} onClick={() => cancel.mutate()} size="xs" variant="subtle">
					{t('desktop.support.discard')}
				</Button>
			</Group>
		</Group>
	);
}

export function PendingReports() {
	const { t } = useTranslation();
	const reports = useQuery(pendingReportsQuery).data ?? [];
	if (reports.length === 0) {
		return null;
	}
	return (
		<Stack gap="xs">
			<Text fw={500} size="sm">
				{t('desktop.support.pendingTitle')}
			</Text>
			{reports.map((report) => (
				<PendingRow key={report.reportId} report={report} />
			))}
		</Stack>
	);
}

export function PolicyTable({ policies }: { policies: PolicyRow[] }) {
	const { t } = useTranslation();
	return (
		<Stack gap="xs">
			<Text c="stone.7" size="xs">
				{t('desktop.support.policiesHelp')}
			</Text>
			<Table fz="sm" verticalSpacing={4}>
				<Table.Thead>
					<Table.Tr>
						<Table.Th>{t('desktop.support.policy')}</Table.Th>
						<Table.Th>{t('desktop.support.value')}</Table.Th>
						<Table.Th>{t('desktop.support.source')}</Table.Th>
					</Table.Tr>
				</Table.Thead>
				<Table.Tbody>
					{policies.map((row) => (
						<Table.Tr key={row.name}>
							<Table.Td>{row.name}</Table.Td>
							<Table.Td style={{ overflowWrap: 'anywhere' }}>{row.value ?? t('desktop.support.notSet')}</Table.Td>
							<Table.Td>{row.source && t(SOURCES[row.source])}</Table.Td>
						</Table.Tr>
					))}
				</Table.Tbody>
			</Table>
		</Stack>
	);
}
