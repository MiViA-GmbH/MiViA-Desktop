import { Button } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';

export function RelocateButton({ folderId, size }: { folderId: string; size?: 'xs' | 'sm' }) {
	const { t } = useTranslation();
	const relocate = useMutation({
		mutationFn: async () => {
			const pick = await commands.pickFolder('input');
			if (pick) {
				await commands.relocateFolder(folderId, pick.pickId);
			}
		},
	});
	return (
		<Button loading={relocate.isPending} onClick={() => relocate.mutate()} size={size ?? 'sm'} variant="default">
			{t('desktop.folder.relocate')}
		</Button>
	);
}
