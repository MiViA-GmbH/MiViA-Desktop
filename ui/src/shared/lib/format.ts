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

export function sameDay(first: string, second: string): boolean {
	return new Date(first).toDateString() === new Date(second).toDateString();
}
