import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const foldersQuery = queryOptions({ queryKey: ['folders'], queryFn: () => commands.listFolders() });
