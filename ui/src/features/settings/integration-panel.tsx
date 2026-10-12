import '../../shared/ui/forms.css';

import { Switch } from '@mantine/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { integrationQuery } from '../../entities/setup/queries';
import { commands } from '../../shared/ipc';
import type { IntegrationView } from '../../shared/ipc/bindings';

type Change = { autostart: boolean | null; explorer: boolean | null };

function useIntegration() {
	const client = useQueryClient();
	const view = useQuery(integrationQuery).data;
	const set = useMutation({
		mutationFn: (change: Change) => commands.setIntegration(change.autostart, change.explorer),
		onSuccess: (next: IntegrationView) => client.setQueryData(integrationQuery.queryKey, next),
	});
	return { view, set };
}

function lockNote(locked: boolean | null, t: ReturnType<typeof useTranslation>['t']): string | undefined {
	return locked === null ? undefined : t('desktop.wizard.common.policy');
}

export function AutostartSwitch() {
	const { t } = useTranslation();
	const { view, set } = useIntegration();
	if (!view) {
		return null;
	}
	const off = view.autostart === 'turnedOffInTaskManager';
	return (
		<Switch
			checked={view.autostart === 'on'}
			description={off ? t('desktop.integration.autostart.turnedOff') : lockNote(view.locks.autostart, t)}
			disabled={!view.available || off || view.locks.autostart !== null}
			label={t('desktop.integration.autostart.label')}
			onChange={(event) => set.mutate({ autostart: event.currentTarget.checked, explorer: null })}
		/>
	);
}

export function ExplorerSwitch() {
	const { t } = useTranslation();
	const { view, set } = useIntegration();
	if (!view) {
		return null;
	}
	const help = view.available ? t('desktop.integration.explorer.help') : t('desktop.integration.explorer.unavailable');
	return (
		<Switch
			checked={view.explorer}
			description={[lockNote(view.locks.explorer, t), help].filter(Boolean).join(' ')}
			disabled={!view.available || view.locks.explorer !== null}
			label={t('desktop.integration.explorer.label')}
			onChange={(event) => set.mutate({ autostart: null, explorer: event.currentTarget.checked })}
		/>
	);
}
