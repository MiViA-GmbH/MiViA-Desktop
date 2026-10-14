import { Anchor, Checkbox, Stack, Text } from '@mantine/core';
import { lazy, type ReactNode, Suspense, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { SetupOption, SetupView } from '../../shared/ipc/bindings';
import { HelpLabel } from '../../shared/ui/help-label';
import { has, previousStep } from './steps';
import { useAdvance } from './use-advance';
import { useGo, useSave, withOption } from './use-setup';
import { WizardFrame } from './wizard-frame';

type ChoiceProps = {
	view: SetupView;
	option: SetupOption;
	locked: boolean | null;
	label: string;
	help: string;
	extra?: ReactNode;
};

function Choice({ view, option, locked, label, help, extra }: ChoiceProps) {
	const { t } = useTranslation();
	const { change } = useSave(view);
	return (
		<Checkbox
			aria-label={label}
			checked={locked ?? has(view, option)}
			description={
				<Stack align="flex-start" gap={4} component="span">
					{locked !== null && t('desktop.wizard.common.policy')}
					{extra}
				</Stack>
			}
			disabled={locked !== null}
			label={<HelpLabel help={help} label={label} />}
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
			<Anchor component="button" onClick={() => setOpen(true)} size="sm" type="button">
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
				help={t('desktop.integration.autostart.help')}
				label={t('desktop.integration.autostart.label')}
				locked={view.locks.autostart}
				option="autostart"
				view={view}
			/>
			<Choice
				help={t('desktop.integration.explorer.help')}
				label={t('desktop.integration.explorer.label')}
				locked={view.locks.explorer}
				option="explorer"
				view={view}
			/>
			<Choice
				extra={<WhatIsSent />}
				help={t('desktop.privacy.telemetryHelp')}
				label={t('desktop.privacy.telemetry')}
				locked={view.locks.telemetry}
				option="telemetry"
				view={view}
			/>
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
