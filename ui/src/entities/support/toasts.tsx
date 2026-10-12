import { Button, Stack, Text } from '@mantine/core';
import type { NotificationData } from '@mantine/notifications';
import type { QueryClient } from '@tanstack/react-query';
import i18n, { type ParseKeys } from 'i18next';

import { commands, isAppError } from '../../shared/ipc';
import type { SendOutcome } from '../../shared/ipc/bindings';
import { requestReport, SUPPORT } from './queries';

const MESSAGES = {
	reportChanged: 'desktop.errors.reportChanged',
	reportMissing: 'desktop.errors.reportMissing',
	stillAnalyzing: 'desktop.errors.stillAnalyzing',
	notReplaceable: 'desktop.errors.notReplaceable',
	changedAfterMove: 'desktop.errors.changedAfterMove',
	notMoved: 'desktop.errors.notMoved',
	renameLocked: 'desktop.errors.renameLocked',
	noImages: 'desktop.errors.noImages',
	tooMany: 'desktop.errors.tooMany',
	backupInvalid: 'desktop.errors.backupInvalid',
	pickExpired: 'desktop.errors.pickExpired',
	reportExpired: 'desktop.errors.reportExpired',
	reportingOff: 'desktop.errors.reportingOff',
	noDownloads: 'desktop.errors.noDownloads',
} as const;

const NAMED = {
	reportChanged: 'desktop.errors.reportChangedName',
	reportMissing: 'desktop.errors.reportMissingName',
} as const;

const OUTCOMES = {
	sent: 'desktop.support.sent',
	queued: 'desktop.support.queued',
	saveAndEmail: 'desktop.support.notSent',
} as const;

const TITLES = {
	sent: 'desktop.toast.reportSent',
	queued: 'desktop.toast.reportQueued',
	saveAndEmail: 'desktop.toast.notSent',
} as const;

function isKnown(code: string | null | undefined): code is keyof typeof MESSAGES {
	return code !== null && code !== undefined && code in MESSAGES;
}

export function failureMessage(error: unknown): string {
	const code = isAppError(error) ? error.code : null;
	const fields = isAppError(error) ? error.fields : null;
	const missing = fields?.missing ?? '';
	const name = fields?.name;
	if (name && (code === 'reportChanged' || code === 'reportMissing')) {
		return i18n.t(NAMED[code], { name });
	}
	return i18n.t(isKnown(code) ? MESSAGES[code] : 'desktop.errors.actionFailed', { missing });
}

export function outcomeMessage(outcome: SendOutcome): string {
	return i18n.t(OUTCOMES[outcome.kind], { reference: outcome.reference });
}

let toastCount = 0;

function notify(data: NotificationData): void {
	import('@mantine/notifications').then(({ notifications }) => notifications.show(data));
}

const alert = () => ({ color: 'red', role: 'alert', title: i18n.t('desktop.errors.title') }) as const;
const STATUS = { role: 'status' } as const;

export function notifyStatus(title: ParseKeys, message: string, extra: Partial<NotificationData> = {}): void {
	notify({ title: i18n.t(title), message, ...STATUS, ...extra });
}

function notifyAction(message: string, action: string, run: () => void, style: Partial<NotificationData>): void {
	toastCount += 1;
	const id = `support-action-${toastCount}`;
	const click = () => {
		import('@mantine/notifications').then(({ notifications }) => notifications.hide(id));
		run();
	};
	const body = (
		<Stack align="flex-start" gap="xs">
			<Text size="sm">{message}</Text>
			<Button onClick={click} size="xs" variant="default">
				{action}
			</Button>
		</Stack>
	);
	notify({ id, message: body, autoClose: false, ...style });
}

function reportableSignature(error: unknown): string | null {
	if (isAppError(error) && error.reportable) {
		return error.signature;
	}
	return null;
}

function claimOffer(signature: string | null): Promise<boolean> {
	if (signature === null) {
		return Promise.resolve(false);
	}
	return commands.claimReportOffer(signature).catch(() => false);
}

export function showFailure(error: unknown, client: QueryClient): void {
	const message = failureMessage(error);
	const signature = reportableSignature(error);
	claimOffer(signature).then((offered) => {
		if (offered) {
			const open = () => requestReport(client, { trigger: 'error', signature });
			notifyAction(message, i18n.t('desktop.support.reportProblem'), open, alert());
		} else {
			notify({ message, ...alert() });
		}
	});
}

export function showSaved(client: QueryClient): void {
	notifyStatus('desktop.toast.reportSaved', i18n.t('desktop.support.saved'));
	client.invalidateQueries({ queryKey: SUPPORT });
}

export function saveReport(reportId: string, client: QueryClient): Promise<void> {
	return commands.saveProblemReport(reportId).then(
		() => showSaved(client),
		(error: unknown) => showFailure(error, client),
	);
}

export function showOutcome(reportId: string, outcome: SendOutcome, client: QueryClient): void {
	client.invalidateQueries({ queryKey: SUPPORT });
	const message = outcomeMessage(outcome);
	if (outcome.kind === 'saveAndEmail') {
		notifyAction(message, i18n.t('desktop.support.saveAndEmail'), () => saveReport(reportId, client), {
			title: i18n.t(TITLES.saveAndEmail),
			...STATUS,
		});
	} else {
		notifyStatus(TITLES[outcome.kind], message);
	}
}
