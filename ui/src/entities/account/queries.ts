import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const accountQuery = queryOptions({ queryKey: ['account'], queryFn: () => commands.getAccount() });

export const localeQuery = queryOptions({ queryKey: ['locale'], queryFn: () => commands.getLocale() });
