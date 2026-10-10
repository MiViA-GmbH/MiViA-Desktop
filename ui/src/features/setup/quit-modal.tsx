import { Button, Group, Modal, Text } from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { QUIT_REQUESTED } from '../../entities/setup/queries';
import { commands } from '../../shared/ipc';

export function QuitModal() {
	const { t } = useTranslation();
	const client = useQueryClient();
	const opened = useQuery<boolean>({ queryKey: QUIT_REQUESTED, queryFn: () => false, enabled: false }).data ?? false;
	const close = () => client.setQueryData(QUIT_REQUESTED, false);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.wizard.quit.continue') }}
			onClose={close}
			opened={opened}
			title={t('desktop.wizard.quit.title')}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.quit.text')}
			</Text>
			<Group justify="flex-end" mt="lg">
				<Button onClick={() => commands.quitApp()} variant="default">
					{t('desktop.wizard.quit.close')}
				</Button>
				<Button data-autofocus onClick={close}>
					{t('desktop.wizard.quit.continue')}
				</Button>
			</Group>
		</Modal>
	);
}
