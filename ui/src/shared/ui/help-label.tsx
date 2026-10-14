import { ActionIcon, Group, Popover, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';

import { HelpIcon } from './icons';

export function HelpLabel({ label, help }: { label: string; help: string }) {
	const { t } = useTranslation();
	return (
		<Group component="span" display="inline-flex" gap={4} wrap="nowrap">
			<span>{label}</span>
			<Popover position="bottom-start" returnFocus shadow="md" trapFocus width={300} withArrow>
				<Popover.Target>
					<ActionIcon
						aria-label={t('desktop.help.moreAbout', { label })}
						color="stone"
						size="md"
						type="button"
						variant="subtle"
					>
						<HelpIcon size={16} />
					</ActionIcon>
				</Popover.Target>
				<Popover.Dropdown>
					<Text size="sm">{help}</Text>
				</Popover.Dropdown>
			</Popover>
		</Group>
	);
}
