import type { AppError } from './bindings';

export { commands, events } from './bindings';

export function isAppError(error: unknown): error is AppError {
	const object = typeof error === 'object' && error !== null;
	return object && 'kind' in error && 'signature' in error;
}
