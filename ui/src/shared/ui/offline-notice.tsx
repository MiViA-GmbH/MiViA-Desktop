import { Button, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { Notice } from './notice';

type OfflineNoticeProps = { details: string; host: string; autoRetry?: boolean };

export function OfflineNotice({ details, host, autoRetry = false }: OfflineNoticeProps) {
	const { t } = useTranslation();
	return (
		<Notice tone="warn">
			<Stack align="flex-start" gap="xs">
				<Text inherit>
					{t('desktop.wizard.key.offline', { host })}
					{autoRetry && ` ${t('desktop.wizard.key.autoRetry')}`}
				</Text>
				<Button onClick={() => navigator.clipboard.writeText(details)} size="xs" variant="default">
					{t('desktop.wizard.key.copyForIt')}
				</Button>
			</Stack>
		</Notice>
	);
}
