import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const BACKUPS = ['backups'] as const;

export const backupsQuery = queryOptions({ queryKey: BACKUPS, queryFn: () => commands.listBackups() });
