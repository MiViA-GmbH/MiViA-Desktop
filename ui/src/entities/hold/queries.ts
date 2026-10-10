import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';

export const holdsQuery = queryOptions({ queryKey: ['holds'], queryFn: () => commands.listHolds() });
