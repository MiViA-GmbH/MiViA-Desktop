type Check = 'scroll' | 'clip' | 'overlap' | 'colour' | 'filled' | 'target';
type Problem = { check: Check; detail: string };
type Known = { finding: string; check: Check; detail: RegExp; keys: string };

const EXEMPT: Omit<Known, 'keys'>[] = [
	{
		finding: 'F05',
		check: 'scroll',
		detail: /^second vertical scroll in div\.mantine-ScrollArea-viewport "(What is sent|Was gesendet wird)"$/,
	},
	{
		finding: 'F06',
		check: 'colour',
		detail: /^surface rgb\(20, 20, 20\) on section\. "(Image viewer|Bildbetrachter)"$/,
	},
];

const KNOWN: Known[] = [];

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
		if (element.matches('td, th') && wide > 1) {
			add('overlap', `content spills out of ${describe(element)} by ${wide}px`);
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
	const covered = (problem: Problem, known: Omit<Known, 'keys'>) =>
		known.check === problem.check && known.detail.test(problem.detail);
	const unexpected = problems
		.filter((problem) => !EXEMPT.some((exempt) => covered(problem, exempt)))
		.filter((problem) => !applies.some((known) => covered(problem, known)))
		.map((problem) => `${problem.check}: ${problem.detail}`);
	const stale = applies
		.filter((known) => !problems.some((problem) => covered(problem, known)))
		.map((known) => `${known.finding} no longer occurs here, remove this key from KNOWN`);
	return { unexpected, stale };
}
