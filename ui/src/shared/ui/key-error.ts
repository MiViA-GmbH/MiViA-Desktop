import type { TFunction } from 'i18next';

import type { ConnectOutcome } from '../ipc/bindings';

export function outcomeError(outcome: ConnectOutcome | null, host: string, t: TFunction): string | null {
	switch (outcome?.kind) {
		case 'malformed':
			return t('desktop.wizard.key.malformed');
		case 'rejected':
			return t('desktop.wizard.key.rejected');
		case 'accountBlocked':
			return t('desktop.wizard.key.blocked');
		case 'adminScope':
			return t('desktop.wizard.key.adminScope');
		case 'missingScopes':
			return t('desktop.wizard.key.missingScopes');
		case 'offline':
			return t('desktop.wizard.key.offline', { host });
		default:
			return null;
	}
}
