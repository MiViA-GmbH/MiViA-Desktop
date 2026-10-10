import { Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { SetupView } from '../../shared/ipc/bindings';
import { PresetChoice, ReportChoice } from './arrival-choice';
import { previousStep } from './steps';
import { useAdvance } from './use-advance';
import { useGo, useSave, useSetupView } from './use-setup';
import { WizardFrame } from './wizard-frame';

export function SeriesStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { label, loading, advance } = useAdvance(view, 'series');
	const { change } = useSave(view);
	const store = useSetupView();
	const pick = useMutation({ mutationFn: () => commands.pickSetupFolder('reports'), onSuccess: store });
	return (
		<WizardFrame
			onBack={() => go(previousStep(view, 'series'))}
			primary={{ label, onClick: advance, loading }}
			step="series"
			title={t('desktop.wizard.series.title')}
			view={view}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.series.lead')}
			</Text>
			<PresetChoice onChange={(preset) => change({ preset })} preset={view.preset} />
			<Text c="stone.7" size="xs">
				{t('desktop.wizard.series.later')}
			</Text>
			{view.report && (
				<ReportChoice
					legacy={view.legacy?.take === 'all'}
					onMode={(reportMode) => change({ reportMode })}
					onPickCustom={() => pick.mutate()}
					report={view.report}
				/>
			)}
		</WizardFrame>
	);
}
