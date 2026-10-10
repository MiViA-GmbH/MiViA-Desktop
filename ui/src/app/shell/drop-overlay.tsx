import { Center, Overlay, Paper, Text } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { DROP_HOVER } from '../../entities/one-off/queries';
import { SingleImagesIcon } from '../../shared/ui/icons';
import classes from './drop-overlay.module.css';

function useDropHover(): boolean {
	return useQuery<boolean>({ queryKey: DROP_HOVER, queryFn: () => false, enabled: false }).data ?? false;
}

export function DropOverlay() {
	const { t } = useTranslation();
	if (!useDropHover()) {
		return null;
	}
	return (
		<Overlay backgroundOpacity={0.85} color="#fff" fixed zIndex={250}>
			<Center h="100%" p="xl">
				<Paper classNames={classes} p="xl" w="100%" h="100%">
					<Center h="100%">
						<Text c="jade.8" fw={600} size="lg">
							<SingleImagesIcon size={20} stroke={ICON_STROKE} /> {t('desktop.oneOff.dropHere')}
						</Text>
					</Center>
				</Paper>
			</Center>
		</Overlay>
	);
}
