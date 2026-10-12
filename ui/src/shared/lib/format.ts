let locale = 'en';

const cache = new Map<string, Intl.DateTimeFormat>();

function formatter(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
	const key = `${locale}|${JSON.stringify(options)}`;
	const known = cache.get(key);
	if (known) {
		return known;
	}
	const created = new Intl.DateTimeFormat(locale, options);
	cache.set(key, created);
	return created;
}

const TIME = { hour: '2-digit', minute: '2-digit' } as const;

export function setFormatLocale(next: string): void {
	locale = Intl.DateTimeFormat.supportedLocalesOf(next).length > 0 ? next : next.slice(0, 2);
}

export function formatTime(iso: string): string {
	return formatter(TIME).format(new Date(iso));
}

export function formatDateTime(iso: string, now: Date = new Date()): string {
	const date = new Date(iso);
	if (date.toDateString() === now.toDateString()) {
		return formatTime(iso);
	}
	if (date.getFullYear() === now.getFullYear()) {
		return formatter({ day: '2-digit', month: '2-digit', ...TIME }).format(date);
	}
	return formatter({ day: '2-digit', month: '2-digit', year: 'numeric', ...TIME }).format(date);
}

export function formatFullDateTime(iso: string): string {
	return formatter({ day: '2-digit', month: '2-digit', year: 'numeric', ...TIME }).format(new Date(iso));
}

export function formatDay(day: string): string {
	return formatter({ day: '2-digit', month: '2-digit', year: 'numeric' }).format(new Date(`${day}T12:00:00`));
}

export function sameDay(first: string, second: string): boolean {
	return new Date(first).toDateString() === new Date(second).toDateString();
}

const sizes = new Map<string, Intl.NumberFormat>();

export function formatSize(bytes: number): string {
	const large = bytes >= 1024 * 1024;
	const unit = large ? 'megabyte' : 'kilobyte';
	const value = bytes / (large ? 1024 * 1024 : 1024);
	const key = `${locale}|${unit}`;
	const known = sizes.get(key) ?? new Intl.NumberFormat(locale, { style: 'unit', unit, maximumFractionDigits: 1 });
	sizes.set(key, known);
	return known.format(value);
}

export function formatPercent(percent: number): string {
	return new Intl.NumberFormat(locale, { style: 'percent' }).format(percent / 100);
}
