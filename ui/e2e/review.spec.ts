import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { extname, join } from 'node:path';
import { expect, type Locator, type Page, test } from '@playwright/test';

import { audit, explain } from './review-audit';

const NOW = new Date('2026-10-12T10:30:00+02:00');
const SHOTS = 'e2e/shots/review';
const KEY = 'mv-AbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKl';
const LANGS = ['en', 'de'] as const;
const SIZES = [
	{ name: '1100', width: 1100, height: 688 },
	{ name: '760', width: 760, height: 448 },
] as const;

type Lang = (typeof LANGS)[number];
type Texts = Record<string, string>;
type Ctx = { page: Page; t: (key: string) => string };
type State = { id: string; path: string; act?: (ctx: Ctx) => Promise<void>; wizardScrolls?: boolean; busy?: boolean };

function flatten(value: unknown, prefix: string, into: Texts): Texts {
	if (typeof value === 'string') {
		into[prefix] = value;
	} else if (typeof value === 'object' && value !== null) {
		for (const [key, child] of Object.entries(value)) {
			flatten(child, prefix ? `${prefix}.${key}` : key, into);
		}
	}
	return into;
}

function bundle(lang: Lang): Texts {
	const read = (ns: string) => JSON.parse(readFileSync(`src/shared/i18n/locales/${ns}/${lang}.json`, 'utf8'));
	return flatten(read('setup'), '', flatten(read('desktop'), '', {}));
}

const TEXTS = { en: bundle('en'), de: bundle('de') };

mkdirSync(SHOTS, { recursive: true });

function translator(lang: Lang): (key: string) => string {
	return (key) => {
		const text = TEXTS[lang][key] ?? TEXTS.en[key];
		if (text === undefined) {
			throw new Error(`unknown text ${key}`);
		}
		return text.split('{{')[0] ?? text;
	};
}

const button = (c: Ctx, key: string): Locator => c.page.getByRole('button', { name: c.t(key), exact: true });
const visible = (c: Ctx, key: string) => expect(c.page.getByText(c.t(key)).first()).toBeVisible();

async function press(c: Ctx, key: string): Promise<void> {
	await button(c, key).first().click();
}

async function dialog(c: Ctx, key: string): Promise<void> {
	await press(c, key);
	await expect(c.page.getByRole('dialog')).toBeVisible();
}

async function emit(page: Page, event: string, payload: unknown): Promise<void> {
	await page.evaluate(
		({ name, data }) => {
			type Internals = { invoke: (command: string, args: Record<string, unknown>) => Promise<unknown> };
			const internals = Reflect.get(window, '__TAURI_INTERNALS__') as Internals;
			return internals.invoke('plugin:event|emit', { event: name, payload: data });
		},
		{ name: event, data: payload },
	);
}

const wizard = {
	key: async (c: Ctx) => {
		await press(c, 'desktop.wizard.welcome.start');
		await visible(c, 'desktop.wizard.key.title');
	},
	typed: async (c: Ctx, key: string, outcome: string) => {
		await wizard.key(c);
		await c.page.getByLabel(c.t('desktop.wizard.key.label')).fill(key);
		await c.page.keyboard.press('Enter');
		await visible(c, outcome);
	},
	folder: async (c: Ctx) => {
		await wizard.typed(c, KEY, 'desktop.wizard.key.connected');
		await press(c, 'desktop.wizard.common.next');
		await visible(c, 'desktop.wizard.folder.title');
	},
	chosen: async (c: Ctx) => {
		await wizard.folder(c);
		await press(c, 'desktop.wizard.folder.choose');
		await visible(c, 'desktop.wizard.folder.existing_other');
	},
	series: async (c: Ctx) => {
		await wizard.chosen(c);
		await press(c, 'desktop.wizard.common.next');
		await visible(c, 'desktop.wizard.series.title');
	},
	models: async (c: Ctx) => {
		await wizard.series(c);
		await press(c, 'desktop.wizard.common.next');
		await visible(c, 'desktop.wizard.models.title');
	},
	picked: async (c: Ctx) => {
		await wizard.models(c);
		await c.page.getByRole('checkbox', { name: /^(Grain size|Korngröße)/ }).click();
		await expect(c.page.getByRole('combobox', { name: c.t('desktop.wizard.models.customization') })).toBeVisible();
	},
	finish: async (c: Ctx) => {
		await wizard.picked(c);
		await press(c, 'desktop.wizard.common.next');
		await visible(c, 'desktop.wizard.finish.title');
	},
};

const STATES: State[] = [
	{ id: 'o1-overview-hold', path: '/?scenario=hold' },
	{ id: 'o2-overview-calm', path: '/?scenario=calm' },
	{ id: 'o3-overview-network', path: '/?scenario=network' },
	{ id: 'o4-overview-empty', path: '/?scenario=empty' },
	{ id: 'o5-overview-paused', path: '/?scenario=paused' },
	{ id: 'h1-hold-service', path: '/?scenario=hold-serviceUnavailable' },
	{ id: 'h2-hold-license', path: '/?scenario=hold-license' },
	{ id: 'h3-hold-key', path: '/?scenario=hold-accessKey' },
	{ id: 'h4-hold-blocked', path: '/?scenario=hold-accountBlocked' },
	{ id: 'h5-hold-admin', path: '/?scenario=hold-adminScope' },
	{ id: 'h6-hold-too-old', path: '/?scenario=too-old' },
	{ id: 'h7-hold-unsigned', path: '/?scenario=update-unsigned' },
	{
		id: 't1-toast-failure',
		path: '/?scenario=failure',
		act: async (c) => {
			await press(c, 'desktop.folder.latestReport');
			await visible(c, 'desktop.errors.reportMissingName');
		},
	},
	{
		id: 't2-toast-status',
		path: '/settings/support?scenario=calm',
		act: async (c) => {
			await dialog(c, 'desktop.support.deleteLogs');
			await c.page
				.getByRole('dialog')
				.getByRole('button', { name: c.t('desktop.support.deleteConfirm'), exact: true })
				.click();
			await expect(c.page.locator('.mantine-Notification-root')).toBeVisible();
			await expect(c.page.getByRole('dialog')).toBeHidden();
		},
	},
	{
		id: 'x1-drop-overlay',
		path: '/?scenario=calm',
		act: async (c) => {
			await emit(c.page, 'drop-hover', true);
			await visible(c, 'desktop.oneOff.dropHere');
		},
	},
	{ id: 'x2-not-found', path: '/nowhere?scenario=calm' },
	{ id: 'f1-folder-series', path: '/folders/1?scenario=calm' },
	{ id: 'f2-folder-settings', path: '/folders/1/settings?scenario=calm' },
	{ id: 'f3-folder-paused', path: '/folders/4?scenario=calm' },
	{ id: 'f4-folder-missing', path: '/folders/5?scenario=hold' },
	{ id: 'f5-new-folder', path: '/folders/new?scenario=calm' },
	{ id: 's1-series', path: '/series/214?scenario=calm' },
	{ id: 's2-series-preview', path: '/series/214?scenario=preview' },
	{
		id: 's3-series-rename',
		path: '/series/214?scenario=renamable',
		act: (c) => dialog(c, 'desktop.series.rename'),
	},
	{
		id: 's4-series-menu',
		path: '/series/214?scenario=calm',
		act: async (c) => {
			await press(c, 'desktop.history.menu');
			await expect(c.page.getByRole('menu')).toBeVisible();
		},
	},
	{ id: 'p1-preview', path: '/preview/214/1?scenario=preview' },
	{ id: 'p2-preview-offline', path: '/preview/214/1?scenario=preview-offline' },
	{
		id: 'p3-preview-result-image',
		path: '/preview/214/1?scenario=preview',
		act: async (c) => {
			await c.page
				.getByRole('button', { name: /placeholder\.png|GRAIN_/ })
				.first()
				.click();
			await expect(button(c, 'desktop.preview.source')).toBeVisible();
			await c.page.evaluate(() => {
				for (const element of document.querySelectorAll('*')) {
					element.scrollTop = 0;
				}
			});
		},
	},
	{ id: 'y1-history', path: '/history?scenario=calm' },
	{ id: 'y2-history-search', path: '/history?scenario=calm&q=4709' },
	{ id: 'y3-history-empty', path: '/history?scenario=empty' },
	{ id: 'y4-history-filtered-empty', path: '/history?scenario=calm&q=Probe%209999&status=problems' },
	{
		id: 'y5-history-row-menu',
		path: '/history?scenario=calm',
		act: async (c) => {
			await c.page
				.getByRole('row', { name: /Probe 4710/ })
				.getByRole('button', { name: c.t('desktop.history.menu') })
				.click();
			await expect(c.page.getByRole('menu')).toBeVisible();
		},
	},
	{
		id: 'c1-custom-report',
		path: '/history?scenario=calm',
		act: async (c) => {
			await c.page.getByRole('checkbox', { name: /Charge 88-214/ }).check();
			await c.page.getByRole('checkbox', { name: /Probe 4710/ }).check();
			await dialog(c, 'desktop.history.createReport');
			await c.page
				.getByRole('dialog')
				.getByRole('button', { name: c.t('desktop.custom.create'), exact: true })
				.click();
			await visible(c, 'desktop.custom.nameRequired');
		},
	},
	{ id: 'c2-custom-reports', path: '/history?scenario=calm&view=custom' },
	{ id: 'm1-single-images', path: '/single-images?scenario=calm' },
	{ id: 'm2-single-images-empty', path: '/single-images?scenario=empty' },
	{
		id: 'd1-one-off',
		path: '/single-images?scenario=one-off',
		act: (c) => expect(c.page.getByRole('dialog')).toBeVisible(),
	},
	{
		id: 'd2-one-off-reports',
		path: '/single-images?scenario=one-off',
		act: async (c) => {
			await expect(c.page.getByRole('dialog')).toBeVisible();
			await press(c, 'desktop.oneOff.changeReport');
		},
	},
	{ id: 'g1-account', path: '/settings/account?scenario=calm' },
	{ id: 'g2-account-missing', path: '/?scenario=no-account' },
	{
		id: 'g3-sign-out',
		path: '/settings/account?scenario=calm',
		act: (c) => dialog(c, 'desktop.account.signOut.button'),
	},
	{ id: 'g4-general', path: '/settings/general?scenario=calm' },
	{
		id: 'g5-restore-backup',
		path: '/settings/general?scenario=calm',
		act: async (c) => {
			await dialog(c, 'desktop.backup.restore');
			await expect(c.page.getByRole('radio')).toHaveCount(3);
		},
	},
	{ id: 'g6-explorer', path: '/settings/explorer?scenario=calm' },
	{ id: 'u1-updates-current', path: '/settings/updates?scenario=calm' },
	{ id: 'u2-updates-checking', path: '/settings/updates?scenario=update-checking', busy: true },
	{ id: 'u3-updates-available', path: '/settings/updates?scenario=update-available' },
	{ id: 'u4-updates-downloading', path: '/settings/updates?scenario=update-downloading' },
	{ id: 'u5-updates-ready', path: '/settings/updates?scenario=update-ready' },
	{ id: 'u6-updates-failed', path: '/settings/updates?scenario=update-failed' },
	{ id: 'u7-updates-locked', path: '/settings/updates?scenario=update-locked' },
	{ id: 'u8-updates-required', path: '/settings/updates?scenario=update-unsigned' },
	{ id: 'v1-privacy', path: '/settings/privacy?scenario=calm' },
	{ id: 'v2-privacy-locked', path: '/settings/privacy?scenario=privacy-locked' },
	{
		id: 'v3-what-is-sent',
		path: '/settings/privacy?scenario=calm',
		act: async (c) => {
			await dialog(c, 'desktop.privacy.whatIsSent');
			await expect(c.page.getByText('session.start', { exact: true })).toBeVisible();
		},
	},
	{
		id: 'v4-sent-data',
		path: '/settings/privacy?scenario=calm',
		act: async (c) => {
			await dialog(c, 'desktop.privacy.viewSent');
			await expect(c.page.getByRole('dialog').getByText(/10:12/)).toBeVisible();
		},
	},
	{
		id: 'v5-sent-data-empty',
		path: '/settings/privacy?scenario=privacy-empty',
		act: async (c) => {
			await dialog(c, 'desktop.privacy.viewSent');
			await visible(c, 'desktop.privacy.sentEmpty');
		},
	},
	{ id: 'r1-support', path: '/settings/support?scenario=calm' },
	{ id: 'r2-support-off', path: '/settings/support?scenario=reporting-off' },
	{
		id: 'r3-report-dialog',
		path: '/settings/support?scenario=calm',
		act: (c) => dialog(c, 'desktop.support.reportAction'),
	},
	{
		id: 'r4-report-review',
		path: '/settings/support?scenario=calm',
		act: async (c) => {
			await dialog(c, 'desktop.support.reportAction');
			await press(c, 'desktop.support.review');
			await expect(c.page.getByRole('dialog').getByRole('combobox')).toBeVisible();
		},
	},
	{
		id: 'r6-report-file',
		path: '/settings/support?scenario=calm',
		act: async (c) => {
			await dialog(c, 'desktop.support.reportAction');
			await press(c, 'desktop.support.review');
			await c.page.getByRole('dialog').getByRole('combobox').click();
			await c.page.getByRole('option').first().click();
			await expect(c.page.getByRole('dialog').locator('pre')).toBeVisible();
		},
	},
	{
		id: 'r5-delete-logs',
		path: '/settings/support?scenario=calm',
		act: (c) => dialog(c, 'desktop.support.deleteLogs'),
	},
	{ id: 'w01-welcome', path: '/?scenario=setup' },
	{
		id: 'w02-legacy',
		path: '/?scenario=setup-legacy',
		act: async (c) => {
			await press(c, 'desktop.wizard.welcome.start');
			await visible(c, 'desktop.wizard.legacy.title');
		},
	},
	{ id: 'w03-key', path: '/?scenario=setup', act: wizard.key },
	{
		id: 'w04-key-rejected',
		path: '/?scenario=setup',
		act: (c) => wizard.typed(c, 'mv-bad-key-0123456789-0123456789-0123456789', 'desktop.wizard.key.rejected'),
		wizardScrolls: true,
	},
	{
		id: 'w05-key-offline',
		path: '/?scenario=setup',
		act: (c) => wizard.typed(c, `mv-offline${KEY.slice(10)}`, 'desktop.wizard.key.offline'),
		wizardScrolls: true,
	},
	{
		id: 'w06-key-connected',
		path: '/?scenario=setup',
		act: (c) => wizard.typed(c, KEY, 'desktop.wizard.key.connected'),
	},
	{ id: 'w07-folder', path: '/?scenario=setup', act: wizard.folder },
	{
		id: 'w08-folder-required',
		path: '/?scenario=setup',
		act: async (c) => {
			await wizard.folder(c);
			await press(c, 'desktop.wizard.common.next');
			await visible(c, 'desktop.wizard.folder.required');
		},
	},
	{ id: 'w09-folder-chosen', path: '/?scenario=setup', act: wizard.chosen },
	{ id: 'w10-series', path: '/?scenario=setup', act: wizard.series },
	{
		id: 'w11-models-required',
		path: '/?scenario=setup',
		act: async (c) => {
			await wizard.models(c);
			await press(c, 'desktop.wizard.common.next');
			await visible(c, 'desktop.wizard.models.required');
		},
	},
	{ id: 'w12-models-chosen', path: '/?scenario=setup', act: wizard.picked },
	{ id: 'w13-finish', path: '/?scenario=setup', act: wizard.finish },
	{
		id: 'w14-done',
		path: '/?scenario=setup',
		act: async (c) => {
			await wizard.finish(c);
			await press(c, 'desktop.wizard.finish.button');
			await visible(c, 'desktop.wizard.done.title');
		},
	},
	{ id: 'w15-resume', path: '/?scenario=setup-resume' },
	{
		id: 'w16-quit',
		path: '/?scenario=setup-resume',
		act: async (c) => {
			await emit(c.page, 'quit-requested', null);
			await expect(c.page.getByRole('dialog')).toBeVisible();
		},
	},
];

async function open(page: Page, path: string, lang: Lang): Promise<void> {
	await page.clock.setFixedTime(NOW);
	await page.goto(`${path}&lang=${lang}`);
	await page.locator('h2, h3').first().waitFor();
	await page.evaluate(() => document.fonts.ready);
}

test.beforeEach(async ({ context, baseURL }) => {
	await context.route(`${baseURL}/**`, (route) => {
		const { pathname } = new URL(route.request().url());
		const file = join('dist-mock', extname(pathname) ? pathname : 'index.html');
		return existsSync(file) ? route.fulfill({ path: file }) : route.fulfill({ status: 404 });
	});
});

for (const state of STATES) {
	for (const lang of LANGS) {
		for (const size of SIZES) {
			const key = `${state.id}-${lang}-${size.name}`;
			test(`review ${key}`, async ({ page }) => {
				await page.setViewportSize({ width: size.width, height: size.height });
				await open(page, state.path, lang);
				await state.act?.({ page, t: translator(lang) });
				if (!state.busy) {
					await expect(page.locator('.mantine-Loader-root')).toHaveCount(0);
				}
				await page.waitForFunction(() => [...document.images].every((image) => image.complete));
				await page.evaluate(() =>
					Promise.all(
						document
							.getAnimations()
							.filter((animation) => animation.effect?.getTiming().iterations !== Number.POSITIVE_INFINITY)
							.map((animation) => animation.finished.catch(() => animation)),
					),
				);
				await page.waitForFunction(() =>
					[...document.querySelectorAll('form .mantine-ScrollArea-viewport')].every(
						(viewport) =>
							viewport.scrollHeight <= viewport.clientHeight ||
							viewport.parentElement?.querySelector('[data-orientation="vertical"][data-state="visible"]'),
					),
				);
				await page.mouse.move(size.width - 2, size.height - 2);
				await page.screenshot({ path: `${SHOTS}/${key}.png` });
				const wizardScrolls = state.wizardScrolls === true || size.name === '760';
				const problems = await page.evaluate(audit, { wizardScrolls });
				writeFileSync(`${SHOTS}/${key}.json`, JSON.stringify(problems, null, '	'));
				expect(explain(key, problems)).toEqual({ unexpected: [], stale: [] });
			});
		}
	}
}
