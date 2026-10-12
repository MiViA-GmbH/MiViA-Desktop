import { createContext, useContext } from 'react';

export const Labels = createContext<Record<string, string>>({});

export function useLabel(): (key: string | undefined) => string {
	const labels = useContext(Labels);
	return (key) => (key ? (labels[key] ?? key) : '');
}
