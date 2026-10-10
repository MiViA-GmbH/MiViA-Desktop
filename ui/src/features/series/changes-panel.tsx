import { Table, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import type { HistoryEventView } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';
import { Panel } from '../../shared/ui/panel';
import { eventBy, eventDetail, eventTitle } from './change-text';

export function ChangesPanel({ events }: { events: HistoryEventView[] }) {
	const { t } = useTranslation();
	return (
		<Panel title={t('desktop.changes.title')}>
			{events.length === 0 && (
				<Text c="stone.7" size="sm">
					{t('desktop.changes.empty')}
				</Text>
			)}
			{events.length > 0 && (
				<Table layout="fixed" verticalSpacing="xs">
					<Table.Tbody>
						{events.map((event) => (
							<Table.Tr key={event.id}>
								<Table.Td c="stone.7" fz="sm" w={130}>
									{formatDateTime(event.at)}
								</Table.Td>
								<Table.Td>
									<Text fw={500} size="sm">
										{eventTitle(event, t)}
									</Text>
									<Text c="stone.7" size="xs" truncate>
										{eventDetail(event, t)}
									</Text>
								</Table.Td>
								<Table.Td c="stone.7" fz="sm" w={180}>
									<Text inherit truncate>
										{eventBy(event, t)}
									</Text>
								</Table.Td>
							</Table.Tr>
						))}
					</Table.Tbody>
				</Table>
			)}
		</Panel>
	);
}
