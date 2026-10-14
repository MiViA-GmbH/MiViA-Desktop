import { Button, Stack, Text } from '@mantine/core';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { updateQuery } from '../../entities/update/queries';
import { commands } from '../../shared/ipc';

export function AboutPanel() {
	const { t } = useTranslation();
	const version = useQuery(updateQuery).data?.current;
	const open = useMutation({ mutationFn: () => commands.openTarget({ kind: 'link', link: 'notices' }) });
	return (
		<Stack align="flex-start" gap="md">
			<Stack gap={2}>
				{version && <Text size="sm">{t('desktop.update.current', { version })}</Text>}
				<Text c="stone.7" size="sm">
					{t('desktop.about.copyright')}
				</Text>
			</Stack>
			<Button loading={open.isPending} onClick={() => open.mutate()} variant="default">
				{t('desktop.about.notices')}
			</Button>
		</Stack>
	);
}
