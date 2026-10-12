import '../../shared/ui/forms.css';

import { Accordion, Code, Modal, ScrollArea, Stack, Text } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import type { SentBatchView } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';
import { noteViewed, sentQuery } from './queries';

function Batch({ batch }: { batch: SentBatchView }) {
	const { t } = useTranslation();
	const label = t('desktop.privacy.sent', { time: formatDateTime(batch.sentAt), count: batch.events });
	return (
		<Accordion.Item value={batch.batchId}>
			<Accordion.Control>{label}</Accordion.Control>
			<Accordion.Panel>
				<ScrollArea.Autosize mah={260} type="auto" viewportProps={{ tabIndex: 0, 'aria-label': label }}>
					<Code block style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>
						{batch.json}
					</Code>
				</ScrollArea.Autosize>
			</Accordion.Panel>
		</Accordion.Item>
	);
}

export function SentDataDialog({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation();
	const batches = useQuery(sentQuery).data;
	useEffect(() => noteViewed('viewSentData'), []);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.privacy.close') }}
			onClose={onClose}
			opened
			size="xl"
			title={t('desktop.privacy.viewSent')}
		>
			<Stack gap="md">
				<Text size="sm">{t('desktop.privacy.serverAdds')}</Text>
				{batches?.length === 0 && <Text size="sm">{t('desktop.privacy.sentEmpty')}</Text>}
				{batches && batches.length > 0 && (
					<>
						<Text fw={500} size="sm">
							{t('desktop.privacy.sentTitle')}
						</Text>
						<Accordion>
							{batches.map((batch) => (
								<Batch batch={batch} key={batch.batchId} />
							))}
						</Accordion>
					</>
				)}
			</Stack>
		</Modal>
	);
}
