type Check = 'scroll' | 'clip' | 'colour' | 'filled' | 'target';
type Problem = { check: Check; detail: string };
type Known = { finding: string; check: Check; detail: RegExp; keys: string };

const KNOWN: Known[] = [
	{
		finding: 'F01',
		check: 'clip',
		detail: /Badge-label/,
		keys: 'c1-custom-report-de-1100 c1-custom-report-de-760 f1-folder-series-de-760 h1-hold-service-de-1100 h1-hold-service-de-760 h2-hold-license-de-1100 h2-hold-license-de-760 h3-hold-key-de-1100 h3-hold-key-de-760 h4-hold-blocked-de-1100 h4-hold-blocked-de-760 h5-hold-admin-de-1100 h5-hold-admin-de-760 h6-hold-too-old-de-1100 h6-hold-too-old-de-760 h7-hold-unsigned-de-1100 h7-hold-unsigned-de-760 o1-overview-hold-de-1100 o1-overview-hold-de-760 o2-overview-calm-de-1100 o2-overview-calm-de-760 o3-overview-network-de-1100 o3-overview-network-de-760 o5-overview-paused-de-1100 o5-overview-paused-de-760 s1-series-de-1100 s1-series-de-760 s1-series-en-760 s2-series-preview-de-1100 s2-series-preview-de-760 s2-series-preview-en-760 s3-series-rename-de-1100 s3-series-rename-de-760 s3-series-rename-en-760 s4-series-menu-de-1100 s4-series-menu-de-760 s4-series-menu-en-760 t1-toast-failure-de-1100 t1-toast-failure-de-760 x1-drop-overlay-de-1100 x1-drop-overlay-de-760 y1-history-de-1100 y1-history-de-760 y2-history-search-de-1100 y2-history-search-de-760 y5-history-row-menu-de-1100 y5-history-row-menu-de-760',
	},
	{
		finding: 'F02',
		check: 'clip',
		detail: /Button-label/,
		keys: 'f1-folder-series-de-1100 f1-folder-series-de-760 f1-folder-series-en-1100 f1-folder-series-en-760 f2-folder-settings-de-1100 f2-folder-settings-de-760 f2-folder-settings-en-1100 f2-folder-settings-en-760 f3-folder-paused-de-760 f3-folder-paused-en-760 f4-folder-missing-de-760 f4-folder-missing-en-760 o5-overview-paused-de-760',
	},
	{
		finding: 'F03',
		check: 'clip',
		detail: /NavLink-body/,
		keys: 'c1-custom-report-de-1100 c2-custom-reports-de-1100 d1-one-off-de-1100 f1-folder-series-de-1100 f2-folder-settings-de-1100 f3-folder-paused-de-1100 f4-folder-missing-de-1100 f5-new-folder-de-1100 g1-account-de-1100 g2-account-missing-de-1100 g3-sign-out-de-1100 g4-general-de-1100 g5-restore-backup-de-1100 g6-explorer-de-1100 h1-hold-service-de-1100 h2-hold-license-de-1100 h3-hold-key-de-1100 h4-hold-blocked-de-1100 h5-hold-admin-de-1100 h6-hold-too-old-de-1100 h7-hold-unsigned-de-1100 m1-single-images-de-1100 m2-single-images-empty-de-1100 o1-overview-hold-de-1100 o2-overview-calm-de-1100 o3-overview-network-de-1100 o4-overview-empty-de-1100 o5-overview-paused-de-1100 p1-preview-de-1100 p2-preview-offline-de-1100 p3-preview-result-image-de-1100 r1-support-de-1100 r2-support-off-de-1100 r3-report-dialog-de-1100 r4-report-review-de-1100 r5-delete-logs-de-1100 s1-series-de-1100 s2-series-preview-de-1100 s3-series-rename-de-1100 s4-series-menu-de-1100 t1-toast-failure-de-1100 t2-toast-status-de-1100 u1-updates-current-de-1100 u2-updates-checking-de-1100 u3-updates-available-de-1100 u4-updates-downloading-de-1100 u5-updates-ready-de-1100 u6-updates-failed-de-1100 u7-updates-locked-de-1100 u8-updates-required-de-1100 v1-privacy-de-1100 v2-privacy-locked-de-1100 v3-what-is-sent-de-1100 v4-sent-data-de-1100 v5-sent-data-empty-de-1100 x1-drop-overlay-de-1100 y1-history-de-1100 y2-history-search-de-1100 y3-history-empty-de-1100 y4-history-filtered-empty-de-1100 y5-history-row-menu-de-1100',
	},
	{
		finding: 'F04',
		check: 'clip',
		detail: /SegmentedControl-root/,
		keys: 'c1-custom-report-de-760 c1-custom-report-en-760 y1-history-de-760 y1-history-en-760 y2-history-search-de-760 y2-history-search-en-760 y3-history-empty-de-760 y3-history-empty-en-760 y4-history-filtered-empty-de-760 y4-history-filtered-empty-en-760 y5-history-row-menu-de-760 y5-history-row-menu-en-760',
	},
	{
		finding: 'F05',
		check: 'scroll',
		detail: /second vertical scroll/,
		keys: 'c1-custom-report-de-760 c1-custom-report-en-760 d1-one-off-de-1100 d1-one-off-de-760 d1-one-off-en-1100 d1-one-off-en-760 r3-report-dialog-de-760 r3-report-dialog-en-760 r4-report-review-de-1100 r4-report-review-de-760 r4-report-review-en-1100 r4-report-review-en-760 v3-what-is-sent-de-1100 v3-what-is-sent-de-760 v3-what-is-sent-en-1100 v3-what-is-sent-en-760',
	},
	{
		finding: 'F06',
		check: 'colour',
		detail: /surface rgb\(20, 20, 20\)/,
		keys: 'p1-preview-de-1100 p1-preview-de-760 p1-preview-en-1100 p1-preview-en-760 p3-preview-result-image-de-1100 p3-preview-result-image-de-760 p3-preview-result-image-en-1100 p3-preview-result-image-en-760',
	},
	{
		finding: 'F07',
		check: 'colour',
		detail: /surface rgb\(222, 255, 255\)/,
		keys: 'c1-custom-report-de-1100 c1-custom-report-de-760 c1-custom-report-en-1100 c1-custom-report-en-760',
	},
	{
		finding: 'F08',
		check: 'colour',
		detail: /orange or red rgb\(250, 82, 82\) on button/,
		keys: 'g3-sign-out-de-1100 g3-sign-out-de-760 g3-sign-out-en-1100 g3-sign-out-en-760 r5-delete-logs-de-1100 r5-delete-logs-de-760 r5-delete-logs-en-1100 r5-delete-logs-en-760',
	},
	{
		finding: 'F09',
		check: 'colour',
		detail: /orange or red rgb\(240, 62, 62\) on p/,
		keys: 'w08-folder-required-de-1100 w08-folder-required-de-760 w08-folder-required-en-1100 w08-folder-required-en-760',
	},
	{
		finding: 'F10',
		check: 'target',
		detail: /ActionIcon-root/,
		keys: 's1-series-de-1100 s1-series-de-760 s1-series-en-1100 s1-series-en-760 s2-series-preview-de-1100 s2-series-preview-de-760 s2-series-preview-en-1100 s2-series-preview-en-760 s3-series-rename-de-1100 s3-series-rename-de-760 s3-series-rename-en-1100 s3-series-rename-en-760 s4-series-menu-de-1100 s4-series-menu-de-760 s4-series-menu-en-1100 s4-series-menu-en-760',
	},
	{
		finding: 'F11',
		check: 'target',
		detail: /22px high button\.mantine-Button-root/,
		keys: 'f1-folder-series-de-1100 f1-folder-series-de-760 f1-folder-series-en-1100 f1-folder-series-en-760 h1-hold-service-de-1100 h1-hold-service-de-760 h1-hold-service-en-1100 h1-hold-service-en-760 h2-hold-license-de-1100 h2-hold-license-de-760 h2-hold-license-en-1100 h2-hold-license-en-760 h3-hold-key-de-1100 h3-hold-key-de-760 h3-hold-key-en-1100 h3-hold-key-en-760 h4-hold-blocked-de-1100 h4-hold-blocked-de-760 h4-hold-blocked-en-1100 h4-hold-blocked-en-760 h5-hold-admin-de-1100 h5-hold-admin-de-760 h5-hold-admin-en-1100 h5-hold-admin-en-760 h6-hold-too-old-de-1100 h6-hold-too-old-de-760 h6-hold-too-old-en-1100 h6-hold-too-old-en-760 h7-hold-unsigned-de-1100 h7-hold-unsigned-de-760 h7-hold-unsigned-en-1100 h7-hold-unsigned-en-760 o1-overview-hold-de-1100 o1-overview-hold-de-760 o1-overview-hold-en-1100 o1-overview-hold-en-760 o2-overview-calm-de-1100 o2-overview-calm-de-760 o2-overview-calm-en-1100 o2-overview-calm-en-760 o3-overview-network-de-1100 o3-overview-network-de-760 o3-overview-network-en-1100 o3-overview-network-en-760 o5-overview-paused-de-1100 o5-overview-paused-de-760 o5-overview-paused-en-1100 o5-overview-paused-en-760 t1-toast-failure-de-1100 t1-toast-failure-de-760 t1-toast-failure-en-1100 t1-toast-failure-en-760 x1-drop-overlay-de-1100 x1-drop-overlay-de-760 x1-drop-overlay-en-1100 x1-drop-overlay-en-760',
	},
	{
		finding: 'F12',
		check: 'target',
		detail: /UnstyledButton-root/,
		keys: 'c1-custom-report-de-1100 c1-custom-report-de-760 c1-custom-report-en-1100 c1-custom-report-en-760 d1-one-off-de-1100 d1-one-off-de-760 d1-one-off-en-1100 d1-one-off-en-760 m1-single-images-de-1100 m1-single-images-de-760 m1-single-images-en-1100 m1-single-images-en-760 y1-history-de-1100 y1-history-de-760 y1-history-en-1100 y1-history-en-760 y2-history-search-de-1100 y2-history-search-de-760 y2-history-search-en-1100 y2-history-search-en-760 y5-history-row-menu-de-1100 y5-history-row-menu-de-760 y5-history-row-menu-en-1100 y5-history-row-menu-en-760',
	},
	{
		finding: 'F13',
		check: 'target',
		detail: /26px high button\.mantine-Button-root/,
		keys: 'c1-custom-report-de-1100 c1-custom-report-de-760 c1-custom-report-en-1100 c1-custom-report-en-760 d1-one-off-de-1100 d1-one-off-de-760 d1-one-off-en-1100 d1-one-off-en-760 p1-preview-de-1100 p1-preview-de-760 p1-preview-en-1100 p1-preview-en-760 p2-preview-offline-de-1100 p2-preview-offline-de-760 p2-preview-offline-en-1100 p2-preview-offline-en-760 p3-preview-result-image-de-1100 p3-preview-result-image-de-760 p3-preview-result-image-en-1100 p3-preview-result-image-en-760 t1-toast-failure-de-1100 t1-toast-failure-de-760 t1-toast-failure-en-1100 t1-toast-failure-en-760',
	},
];

type AuditOptions = { wizardScrolls: boolean };

export function audit({ wizardScrolls }: AuditOptions): Problem[] {
	const problems: Problem[] = [];
	const add = (check: Check, detail: string) => problems.push({ check, detail });
	const elements = [...document.querySelectorAll<HTMLElement>('body *')];
	const describe = (element: Element) => {
		const label = element.getAttribute('aria-label') ?? element.textContent ?? '';
		const role = element.getAttribute('role');
		const kind = [...element.classList].find((name) => /^mantine-[A-Z]/.test(name)) ?? '';
		const name = label.replace(/\s+/g, ' ').trim().slice(0, 48);
		return `${element.tagName.toLowerCase()}${role ? `[${role}]` : ''}.${kind} "${name}"`;
	};
	const shown = (element: Element) => {
		const box = element.getBoundingClientRect();
		return box.width > 2 && box.height > 2 && getComputedStyle(element).visibility !== 'hidden';
	};

	const probe = document.createElement('i');
	document.body.append(probe);
	const rootStyle = getComputedStyle(document.documentElement);
	const resolve = (name: string) => {
		if (!rootStyle.getPropertyValue(name).trim()) {
			return null;
		}
		probe.style.color = `var(${name})`;
		return getComputedStyle(probe).color;
	};
	const SUFFIXES = [...'0123456789'].map((step) => `-${step}`).concat(['-filled', '-light', '-light-color', '-text']);
	const palette = (names: string[]) =>
		new Set(
			names.flatMap((name) =>
				SUFFIXES.map((suffix) => resolve(`--mantine-color-${name}${suffix}`)).filter((value) => value !== null),
			),
		);
	const surfaces = new Set([
		resolve('--mantine-color-jade-8'),
		resolve('--mantine-color-stone-1'),
		resolve('--mantine-color-white'),
	]);
	const warm = palette(['orange', 'red']);
	const foreign = palette(['blue', 'cyan', 'teal', 'green', 'lime', 'yellow', 'grape', 'violet', 'indigo', 'pink']);
	probe.remove();
	const clear = (colour: string) =>
		colour === 'transparent' || /^rgba\(.*, 0\)$/.test(colour) || /\/ 0\)$/.test(colour);

	const doc = document.documentElement;
	if (doc.scrollWidth > doc.clientWidth + 1) {
		add('scroll', `page scrolls horizontally by ${doc.scrollWidth - doc.clientWidth}px`);
	}
	if (doc.scrollHeight > doc.clientHeight + 1) {
		add('scroll', `page scrolls vertically by ${doc.scrollHeight - doc.clientHeight}px`);
	}

	const SURFACE_SKIP =
		'.mantine-Alert-root, .mantine-Button-root, .mantine-ActionIcon-root, .mantine-Badge-root, .mantine-Input-input, .mantine-ThemeIcon-root, .mantine-Progress-root, .mantine-Overlay-root, .mantine-Modal-overlay, .mantine-Tooltip-tooltip, .mantine-Avatar-root, .mantine-Stepper-stepIcon, img, svg, canvas';
	const WARM_OK =
		'.mantine-Badge-root, .mantine-Alert-root, .mantine-AppShell-navbar, .mantine-ThemeIcon-root, .mantine-Notification-root, .mantine-InputWrapper-error, [data-error], [role="alert"], .mantine-Button-root[data-variant="subtle"]';
	const CLIPPED =
		'.mantine-Button-label, .mantine-Badge-label, .mantine-SegmentedControl-root, .mantine-NavLink-body, .mantine-NavLink-label, .mantine-NavLink-description, .mantine-Tabs-tabLabel, .mantine-Stepper-stepLabel, .mantine-Modal-title, .mantine-Notification-title, .mantine-Menu-itemLabel, h1, h2, h3, h4';
	const TARGETS =
		'button, a[href], [role="button"], [role="tab"], [role="menuitem"], .mantine-NavLink-root, .mantine-ActionIcon-root';
	const scrollable = (value: string) => value === 'auto' || value === 'scroll';

	const filled = new Map<Element, number>();
	for (const element of elements) {
		if (!shown(element)) {
			continue;
		}
		const style = getComputedStyle(element);
		const box = element.getBoundingClientRect();

		if (scrollable(style.overflowX) && element.scrollWidth > element.clientWidth + 1) {
			add('scroll', `horizontal scroll in ${describe(element)}`);
		}
		if (scrollable(style.overflowY) && element.scrollHeight > element.clientHeight + 1) {
			const inMain = element.closest('.mantine-AppShell-main') !== null;
			const contained = element.closest('.mantine-Paper-root, .mantine-Modal-root') !== null;
			const content = inMain && !contained && element.matches('.mantine-ScrollArea-viewport');
			const sidebar = element.closest('.mantine-AppShell-navbar') !== null;
			const wizard = !inMain && element.closest('form') !== null && element.closest('.mantine-Modal-root') === null;
			const list = element.closest('.mantine-Combobox-dropdown') !== null;
			const allowed = [content, sidebar, list, wizard && wizardScrolls].some(Boolean);
			if (!allowed) {
				add('scroll', `second vertical scroll in ${describe(element)}`);
			}
		}

		const wide = element.scrollWidth - element.clientWidth;
		const tall = element.scrollHeight - element.clientHeight;
		if (element.matches(CLIPPED) && (wide > 1 || tall > 2)) {
			add('clip', `clipped ${describe(element)} by ${wide}x${tall}px`);
		}

		const bg = style.backgroundColor;
		const painted = !clear(bg) && style.backgroundImage === 'none';
		const large = box.width >= 120 && box.height >= 48;
		const offPalette = !element.closest(SURFACE_SKIP) && !surfaces.has(bg);
		if (painted && large && offPalette) {
			add('colour', `surface ${bg} on ${describe(element)}`);
		}
		const parentColour = element.parentElement ? getComputedStyle(element.parentElement).color : '';
		const border = Number.parseFloat(style.borderTopWidth) > 0 ? style.borderTopColor : '';
		const own = [style.color === parentColour ? '' : style.color, clear(bg) ? '' : bg, border].filter(Boolean);
		for (const colour of own) {
			if (foreign.has(colour)) {
				add('colour', `foreign hue ${colour} on ${describe(element)}`);
			}
			if (warm.has(colour) && !element.closest(WARM_OK)) {
				add('colour', `orange or red ${colour} on ${describe(element)}`);
			}
		}

		if (
			element.matches(
				'.mantine-Button-root:is([data-variant="filled"], :not([data-variant])), .mantine-ActionIcon-root:is([data-variant="filled"], :not([data-variant]))',
			)
		) {
			const region =
				element.closest(
					'.mantine-Modal-content, .mantine-Alert-root, .mantine-Notification-root, .mantine-Paper-root, .mantine-AppShell-main',
				) ?? document.body;
			filled.set(region, (filled.get(region) ?? 0) + 1);
		}

		const textLink =
			style.display === 'inline' ||
			element.matches(
				'.mantine-Anchor-root, a:not(.mantine-Button-root, .mantine-ActionIcon-root, .mantine-NavLink-root, .mantine-Tabs-tab)',
			);
		if (element.matches(TARGETS) && !textLink && element.getAttribute('tabindex') !== '-1') {
			const navbar = element.closest('.mantine-AppShell-navbar') !== null;
			const minimum = navbar ? 40 : 28;
			if (box.height < minimum - 0.5) {
				add('target', `${Math.round(box.height)}px high ${describe(element)}`);
			}
		}
	}
	for (const [region, count] of filled) {
		if (count > 1) {
			add('filled', `${count} filled buttons in ${describe(region).slice(0, 80)}`);
		}
	}
	return problems;
}

export function explain(key: string, problems: Problem[]): { unexpected: string[]; stale: string[] } {
	const applies = KNOWN.filter((known) => known.keys.split(' ').includes(key));
	const covered = (problem: Problem, known: Known) =>
		known.check === problem.check && known.detail.test(problem.detail);
	const unexpected = problems
		.filter((problem) => !applies.some((known) => covered(problem, known)))
		.map((problem) => `${problem.check}: ${problem.detail}`);
	const stale = applies
		.filter((known) => !problems.some((problem) => covered(problem, known)))
		.map((known) => `${known.finding} no longer occurs here, remove this key from KNOWN`);
	return { unexpected, stale };
}
