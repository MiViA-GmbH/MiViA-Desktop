import '../../shared/ui/forms.css';

import { Button, Checkbox, Group, Modal, Radio, Stack, Text, Textarea } from '@mantine/core';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { contactText, type ReportRequest, requestReport } from '../../entities/support/queries';
import { showOutcome, showSaved } from '../../entities/support/toasts';
import { commands } from '../../shared/ipc';
import type { ProblemReportInput, ProblemReportPreview, ReportRange, SupportView } from '../../shared/ipc/bindings';
import { ReportReview } from './report-review';

const DESCRIPTION_LIMIT = 4000;
const RANGES: Record<
	ReportRange,
	'desktop.support.rangeSession' | 'desktop.support.rangeThreeDays' | 'desktop.support.rangeFourteenDays'
> = {
	session: 'desktop.support.rangeSession',
	threeDays: 'desktop.support.rangeThreeDays',
	fourteenDays: 'desktop.support.rangeFourteenDays',
};

function isRange(value: string): value is ReportRange {
	return value in RANGES;
}

type Prepared = { key: string; preview: ProblemReportPreview };

function useReport(input: ProblemReportInput, close: () => void) {
	const client = useQueryClient();
	const [prepared, setPrepared] = useState<Prepared | null>(null);
	const key = JSON.stringify(input);
	const fresh = prepared?.key === key ? prepared.preview : null;
	const prepare = useMutation({
		mutationFn: (value: ProblemReportInput) => commands.prepareProblemReport(value),
		onSuccess: (preview, value) => setPrepared({ key: JSON.stringify(value), preview }),
	});
	const reportId = async () => fresh?.reportId ?? (await commands.prepareProblemReport(input)).reportId;
	const send = useMutation({
		mutationFn: async () => {
			const id = await reportId();
			return { id, outcome: await commands.sendProblemReport(id) };
		},
		onSuccess: ({ id, outcome }) => showOutcome(id, outcome, client),
	});
	const save = useMutation({
		mutationFn: async () => commands.saveProblemReport(await reportId()),
		onSuccess: () => {
			showSaved(client);
			close();
		},
	});
	return { fresh, prepare, send, save };
}

type FormProps = {
	input: ProblemReportInput;
	update: (changes: Partial<ProblemReportInput>) => void;
	reviewing: boolean;
	onReview: () => void;
};

function Fields({ input, update, reviewing, onReview }: FormProps) {
	const { t } = useTranslation();
	return (
		<Group align="flex-start" gap="lg" grow wrap="nowrap">
			<Textarea
				autosize
				data-autofocus
				description={t('desktop.support.descriptionHint')}
				label={t('desktop.support.description')}
				maxLength={DESCRIPTION_LIMIT}
				maxRows={10}
				minRows={4}
				onChange={(event) => update({ description: event.currentTarget.value })}
				value={input.description}
			/>
			<Stack gap="sm">
				<Radio.Group
					label={t('desktop.support.range')}
					onChange={(value) => isRange(value) && update({ range: value })}
					value={input.range}
				>
					<Stack gap="xs" mt={4}>
						{(Object.keys(RANGES) as ReportRange[]).map((range) => (
							<Radio key={range} label={t(RANGES[range])} value={range} />
						))}
					</Stack>
				</Radio.Group>
				<Checkbox
					checked={input.includeNames}
					description={t('desktop.support.namesHint')}
					label={t('desktop.support.names')}
					onChange={(event) => update({ includeNames: event.currentTarget.checked })}
				/>
				<Button loading={reviewing} onClick={onReview} size="xs" style={{ alignSelf: 'flex-start' }} variant="default">
					{t('desktop.support.review')}
				</Button>
			</Stack>
		</Group>
	);
}

type DialogProps = { request: ReportRequest; support: SupportView };

function Body({ request, support }: DialogProps) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const close = () => requestReport(client, null);
	const [input, setInput] = useState<ProblemReportInput>(() => ({
		description: '',
		range: 'threeDays',
		includeNames: false,
		...request,
	}));
	const update = (changes: Partial<ProblemReportInput>) => setInput((current) => ({ ...current, ...changes }));
	const { fresh, prepare, send, save } = useReport(input, close);
	const [reviewing, setReviewing] = useState(false);
	return (
		<Stack gap="md">
			{reviewing && fresh ? (
				<ReportReview preview={fresh} />
			) : (
				<Fields
					input={input}
					onReview={() => {
						setReviewing(true);
						prepare.mutate(input);
					}}
					reviewing={prepare.isPending}
					update={update}
				/>
			)}
			<Text c="stone.7" size="xs">
				{t('desktop.support.disclosure')}
			</Text>
			{!support.canSend && (
				<Text size="sm">{t('desktop.support.sendOff', { contact: contactText(support.contact) })}</Text>
			)}
			<Group gap="sm" justify="flex-end">
				{reviewing && fresh && (
					<Button mr="auto" onClick={() => setReviewing(false)} variant="default">
						{t('desktop.wizard.common.back')}
					</Button>
				)}
				<Button onClick={close} variant="default">
					{t('desktop.custom.cancel')}
				</Button>
				<Button loading={save.isPending} onClick={() => save.mutate()} variant={support.canSend ? 'default' : 'filled'}>
					{t('desktop.support.saveAndEmail')}
				</Button>
				{support.canSend && (
					<Button
						onClick={() => {
							send.mutate();
							close();
						}}
					>
						{t('desktop.support.send')}
					</Button>
				)}
			</Group>
		</Stack>
	);
}

export function ReportDialog({ request, support }: DialogProps) {
	const { t } = useTranslation();
	const client = useQueryClient();
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{t('desktop.support.title')}
		</Text>
	);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.custom.cancel') }}
			onClose={() => requestReport(client, null)}
			opened
			size="xl"
			title={title}
			yOffset="2dvh"
		>
			<Body request={request} support={support} />
		</Modal>
	);
}
