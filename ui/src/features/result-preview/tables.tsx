import { Alert, Table, Text } from '@mantine/core';

import type { DataTableResult, MessageResult, MultiColumnTableResult } from './results';
import { groupItems, keyed, roundValue } from './results';
import { useLabel } from './use-label';

const STATUS_COLOR = { info: 'blue', warning: 'orange', error: 'red' };

export function Message({ result }: { result: MessageResult }) {
	const label = useLabel();
	if (!result.data) {
		throw new Error('message without data');
	}
	const { title, content, status } = result.data;
	return (
		<Alert color={STATUS_COLOR[status]} title={label(`message.${title}`)} variant="outline" w="100%">
			{label(`message.${content}`)}
		</Alert>
	);
}

export function DataTable({ result }: { result: DataTableResult }) {
	const label = useLabel();
	const columns = result.data.some((item) => item.unit) ? 3 : 2;
	return (
		<Table striped withTableBorder>
			<Table.Tbody>
				{groupItems(result.data).map(({ group, items }) => [
					group && (
						<Table.Tr key={`group-${group}`}>
							<Table.Td colSpan={columns} fw={700}>
								{label(group)}
							</Table.Td>
						</Table.Tr>
					),
					...items.map((item) => (
						<Table.Tr key={item.label} title={item.labelTooltip ? label(item.labelTooltip) : undefined}>
							<Table.Td>{label(item.label)}</Table.Td>
							<Table.Td>{roundValue(item.value)}</Table.Td>
							{item.unit && <Table.Td>[{label(item.unit)}]</Table.Td>}
						</Table.Tr>
					)),
				])}
			</Table.Tbody>
		</Table>
	);
}

function cellText(cell: string | number | null, label: (key: string) => string): string {
	if (cell === null) {
		return '';
	}
	return typeof cell === 'number' ? roundValue(cell) : label(cell);
}

export function MultiColumnTable({ result }: { result: MultiColumnTableResult }) {
	const label = useLabel();
	const units = result.units ?? result.unit ?? [];
	return (
		<Table striped withTableBorder>
			<Table.Thead>
				<Table.Tr>
					{(result.headers ?? []).map((header, index) => (
						<Table.Th key={header}>
							{label(header)}
							{units[index] ? <Text component="span"> [{label(units[index])}]</Text> : null}
						</Table.Th>
					))}
				</Table.Tr>
			</Table.Thead>
			<Table.Tbody>
				{keyed(result.data).map(({ id, item: row }) => (
					<Table.Tr key={id}>
						{keyed(row).map(({ id: cellId, item: cell }) => (
							<Table.Td key={cellId}>{cellText(cell, label)}</Table.Td>
						))}
					</Table.Tr>
				))}
			</Table.Tbody>
		</Table>
	);
}
