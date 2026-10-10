import type { TFunction } from 'i18next';

import type { FolderBlock } from '../../shared/ipc/bindings';

export function blockText(block: FolderBlock, t: TFunction): string {
	switch (block.kind) {
		case 'notFound':
			return t('desktop.wizard.folder.notFound');
		case 'nested':
			return t('desktop.wizard.folder.nested', { name: block.name });
		case 'tooBroad':
		case 'insideReports':
			return t('desktop.wizard.folder.tooBroad');
		default:
			return block satisfies never;
	}
}
