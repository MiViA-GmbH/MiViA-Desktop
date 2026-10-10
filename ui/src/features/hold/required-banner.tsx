import { Alert, Anchor, Group, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { ReportButton } from '../../entities/support/report-button';
import { updateQuery } from '../../entities/update/queries';
import { commands } from '../../shared/ipc';
import { TooOldIcon } from '../../shared/ui/icons';
import classes from './hold-banner.module.css';

export function RequiredBanner() {
	const { t } = useTranslation();
	const required = useQuery(updateQuery).data?.required;
	const releases = useMutation({ mutationFn: () => commands.openTarget({ kind: 'link', link: 'releases' }) });
	if (!required || required.signed) {
		return null;
	}
	return (
		<Alert
			bg="orange.0"
			classNames={classes}
			color="orange"
			icon={<TooOldIcon size={20} stroke={ICON_STROKE} />}
			px="xl"
			radius={0}
			role="status"
		>
			<Group gap="md" justify="space-between" wrap="nowrap">
				<Text c="dark.7" size="sm">
					<Text fw={600} inherit span>
						{t('desktop.update.requiredTitle')}
					</Text>{' '}
					{t('desktop.update.requiredText')}{' '}
					<Anchor component="button" inherit onClick={() => releases.mutate()}>
						{t('desktop.update.releases')}
					</Anchor>
				</Text>
				<ReportButton />
			</Group>
		</Alert>
	);
}
