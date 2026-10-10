import { Anchor, Checkbox, Stack, Text } from '@mantine/core';
import { lazy, type ReactNode, Suspense, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { SetupOption, SetupView } from '../../shared/ipc/bindings';
import { has, previousStep } from './steps';
import { useAdvance } from './use-advance';
import { useGo, useSave, withOption } from './use-setup';
import { WizardFrame } from './wizard-frame';

type ChoiceProps = {
	view: SetupView;
	option: SetupOption;
	locked: boolean | null;
	label: string;
	description?: ReactNode;
};

function Choice({ view, option, locked, label, description }: ChoiceProps) {
	const { t } = useTranslation();
	const { change } = useSave(view);
	return (
		<Checkbox
			checked={locked ?? has(view, option)}
			description={locked === null ? description : t('desktop.wizard.common.policy')}
			disabled={locked !== null}
			label={label}
			onChange={(event) => change({ options: withOption(view.options, option, event.currentTarget.checked) })}
		/>
	);
}

const FieldListDialog = lazy(() =>
	import('../../entities/privacy/field-list-dialog').then((module) => ({ default: module.FieldListDialog })),
);

function WhatIsSent() {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	return (
		<>
			<Anchor
				component="button"
				onClick={() => setOpen(true)}
				size="sm"
				style={{ alignSelf: 'flex-start' }}
				type="button"
			>
				{t('desktop.privacy.whatIsSent')}
			</Anchor>
			{open && (
				<Suspense fallback={null}>
					<FieldListDialog onClose={() => setOpen(false)} />
				</Suspense>
			)}
		</>
	);
}

function FinishChoices({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	return (
		<>
			<Choice
				label={t('desktop.wizard.finish.autostart')}
				locked={view.locks.autostart}
				option="autostart"
				view={view}
			/>
			<Choice label={t('desktop.wizard.finish.explorer')} locked={view.locks.explorer} option="explorer" view={view} />
			<Stack gap={4}>
				<Choice
					description={t('desktop.wizard.finish.telemetryNever')}
					label={t('desktop.wizard.finish.telemetry')}
					locked={view.locks.telemetry}
					option="telemetry"
					view={view}
				/>
				<WhatIsSent />
			</Stack>
			<Text c="stone.7" size="sm">
				{t('desktop.wizard.finish.later')}
			</Text>
		</>
	);
}

export function FinishStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { change } = useSave(view);
	const { label, loading, advance } = useAdvance(view, 'finish');
	const back = async () => {
		const step = previousStep(view, 'finish');
		if (has(view, 'skipFolder')) {
			await change({ options: withOption(view.options, 'skipFolder', false) });
		}
		await go(step);
	};
	return (
		<WizardFrame
			onBack={back}
			primary={{ label, onClick: advance, loading }}
			step="finish"
			title={t('desktop.wizard.finish.title')}
			view={view}
		>
			<FinishChoices view={view} />
		</WizardFrame>
	);
}
