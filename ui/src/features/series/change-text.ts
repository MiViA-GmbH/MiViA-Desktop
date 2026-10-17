import type { TFunction } from 'i18next';

import type { HistoryEventView } from '../../shared/ipc/bindings';

const ACTIONS = {
	series_opened: 'desktop.changes.actions.series_opened',
	series_closed: 'desktop.changes.actions.series_closed',
	series_renamed: 'desktop.changes.actions.series_renamed',
	report_saved: 'desktop.changes.actions.report_saved',
	report_updated: 'desktop.changes.actions.report_updated',
	custom_report_created: 'desktop.changes.actions.custom_report_created',
	moved_to_replaced: 'desktop.changes.actions.moved_to_replaced',
	restored_from_replaced: 'desktop.changes.actions.restored_from_replaced',
	attention_cleared: 'desktop.changes.actions.attention_cleared',
	history_exported: 'desktop.changes.actions.history_exported',
	backup_restored: 'desktop.changes.actions.backup_restored',
	deleted: 'desktop.changes.actions.deleted',
	deletion_finished: 'desktop.changes.actions.deletion_finished',
} as const;

const REASONS = {
	quiet_period: 'desktop.changes.reasons.quiet_period',
	cap: 'desktop.changes.reasons.cap',
	manual: 'desktop.changes.reasons.manual',
	restart: 'desktop.changes.reasons.restart',
	account_switch: 'desktop.changes.reasons.account_switch',
	one_off: 'desktop.changes.reasons.one_off',
} as const;

const HOW = {
	opened: 'desktop.changes.how.opened',
	dismissed: 'desktop.changes.how.dismissed',
	kept: 'desktop.changes.how.kept',
} as const;

const TRIGGERS = {
	automatic: 'desktop.changes.triggers.automatic',
	folder: 'desktop.changes.triggers.folder',
	send_to: 'desktop.changes.triggers.sendTo',
} as const;

function lookup<T extends Record<string, string>>(map: T, key: string): T[keyof T] | undefined {
	return key in map ? map[key as keyof T] : undefined;
}

function fields(json: string | null): Record<string, unknown> {
	try {
		const parsed: unknown = JSON.parse(json ?? '{}');
		return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : {};
	} catch {
		return {};
	}
}

function value(record: Record<string, unknown>, key: string): string {
	const found = record[key];
	return typeof found === 'string' ? found : '';
}

export function eventDetail(event: HistoryEventView, t: TFunction): string {
	const before = fields(event.before);
	const after = fields(event.after);
	const reason = lookup(REASONS, value(after, 'reason'));
	const how = lookup(HOW, value(after, 'how'));
	if (event.action === 'series_renamed') {
		return t('desktop.changes.renamed', { before: value(before, 'label'), after: value(after, 'label') });
	}
	if (reason) {
		return t(reason);
	}
	if (how) {
		return t(how);
	}
	return value(after, event.action === 'deleted' ? 'label' : 'path');
}

export function eventTitle(event: HistoryEventView, t: TFunction): string {
	const key = lookup(ACTIONS, event.action);
	return key ? t(key) : event.action;
}

export function eventBy(event: HistoryEventView, t: TFunction): string {
	const key = lookup(TRIGGERS, event.trigger);
	return key ? t(key) : event.windowsUser;
}
