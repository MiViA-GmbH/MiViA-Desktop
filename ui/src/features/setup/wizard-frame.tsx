import '../../shared/ui/forms.css';

import { BackgroundImage, Box, Button, Group, Paper, ScrollArea, Stack, Stepper, Text, Title } from '@mantine/core';
import { MiviaLogoClaimBelow } from '@mivia-ai/theme';
import background from '@mivia-ai/theme/background.jpeg';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { SetupStep, SetupView } from '../../shared/ipc/bindings';
import { Notice } from '../../shared/ui/notice';
import { QuitModal } from './quit-modal';
import { countedSteps, isSkipped, stepperSteps } from './steps';
import { ERROR_KEY, type SetupError } from './use-advance';
import card from './wizard-card.module.css';
import footer from './wizard-footer.module.css';
import layout from './wizard-layout.module.css';
import logo from './wizard-logo.module.css';
import steps from './wizard-steps.module.css';
import heading from './wizard-title.module.css';

const STEP_LABELS = {
	welcome: 'desktop.wizard.steps.key',
	legacy: 'desktop.wizard.steps.legacy',
	key: 'desktop.wizard.steps.key',
	folder: 'desktop.wizard.steps.folder',
	series: 'desktop.wizard.steps.series',
	models: 'desktop.wizard.steps.models',
	finish: 'desktop.wizard.steps.finish',
} as const;

function WizardStepper({ view, step }: { view: SetupView; step: SetupStep }) {
	const { t } = useTranslation();
	const steps = stepperSteps(view);
	return (
		<Stepper
			active={steps.indexOf(step)}
			allowNextStepsSelect={false}
			color="jade"
			iconSize={28}
			orientation="vertical"
			size="sm"
			styles={{ stepBody: { minHeight: 28, justifyContent: 'center', alignSelf: 'flex-start' } }}
		>
			{steps.map((key) => (
				<Stepper.Step
					aria-current={key === step ? 'step' : undefined}
					description={isSkipped(view, key) ? t('desktop.wizard.steps.skipped') : undefined}
					key={key}
					label={t(STEP_LABELS[key])}
					tabIndex={-1}
				/>
			))}
		</Stepper>
	);
}

type Primary = { label: string; onClick: () => void; loading?: boolean; disabled?: boolean };

type FrameProps = {
	view: SetupView;
	step: SetupStep | 'done';
	title: ReactNode;
	primary: Primary;
	onBack?: (() => void) | undefined;
	children: ReactNode;
};

let resumeNoteShown = false;

function Footer({ view, step, primary, onBack }: Omit<FrameProps, 'title' | 'children'>) {
	const { t } = useTranslation();
	const counted = step === 'done' ? [] : countedSteps(view);
	const index = step === 'done' ? -1 : counted.indexOf(step);
	return (
		<Group classNames={footer} gap="md" px={36} py="md" wrap="nowrap">
			{onBack && (
				<Button onClick={onBack} variant="default">
					{t('desktop.wizard.common.back')}
				</Button>
			)}
			{index >= 0 && (
				<Text c="stone.7" size="sm">
					{t('desktop.wizard.common.stepOf', { n: index + 1, m: counted.length })}
				</Text>
			)}
			<Box flex={1} />
			<Button disabled={primary.disabled ?? false} loading={primary.loading ?? false} type="submit">
				{primary.label}
			</Button>
		</Group>
	);
}

type ContentProps = { view: SetupView; step: SetupStep | 'done'; title: ReactNode; children: ReactNode };

function Content({ view, step, title, children }: ContentProps) {
	const { t } = useTranslation();
	const titleRef = useRef<HTMLHeadingElement>(null);
	const [resumed] = useState(() => view.resumed && !resumeNoteShown);
	const failed = useQuery<SetupError | null>({ queryKey: ERROR_KEY, queryFn: () => null, enabled: false }).data;
	useEffect(() => {
		resumeNoteShown = true;
		if (step) {
			titleRef.current?.focus();
		}
	}, [step]);
	return (
		<ScrollArea flex={1} type="auto">
			<Stack gap="md" px={36} py="lg">
				{resumed && <Notice tone="info">{t('desktop.wizard.resume.note')}</Notice>}
				{view.imagesSent > 0 && <Notice tone="info">{t('desktop.oneOff.waitingForSetup')}</Notice>}
				<Title classNames={heading} order={3} ref={titleRef} tabIndex={-1}>
					{title}
				</Title>
				{failed?.step === step && <Notice tone="error">{failed.message}</Notice>}
				{children}
			</Stack>
		</ScrollArea>
	);
}

export function WizardFrame({ view, step, title, primary, onBack, children }: FrameProps) {
	const withSteps = step !== 'welcome' && step !== 'done';
	return (
		<BackgroundImage classNames={layout} src={background}>
			<Group classNames={logo}>
				<MiviaLogoClaimBelow aria-label="MiViA" role="img" variant="white" width={140} />
			</Group>
			<Paper classNames={card} radius="md" withBorder>
				{withSteps && (
					<Stack bg="stone.1" classNames={steps} p="lg">
						<WizardStepper step={step} view={view} />
					</Stack>
				)}
				<Stack
					component="form"
					flex={1}
					gap={0}
					miw={0}
					onSubmit={(event) => {
						event.preventDefault();
						primary.onClick();
					}}
				>
					<Content step={step} title={title} view={view}>
						{children}
					</Content>
					<Footer onBack={onBack} primary={primary} step={step} view={view} />
				</Stack>
			</Paper>
			<QuitModal />
		</BackgroundImage>
	);
}
