import { AppShell, ScrollArea, Stack } from '@mantine/core';
import { useMediaQuery } from '@mantine/hooks';
import { Outlet } from '@tanstack/react-router';

import { HoldBanner } from '../../features/hold/hold-banner';
import { DropOverlay } from './drop-overlay';
import { OneOffHost } from './one-off-host';
import { ReportHost } from './report-host';
import { Sidebar } from './sidebar';

export function Shell() {
	const rail = useMediaQuery('(max-width: 959px)') ?? false;
	return (
		<AppShell navbar={{ width: rail ? 64 : 248, breakpoint: 0 }} withBorder={false}>
			<AppShell.Navbar bg="jade.8">
				<Sidebar rail={rail} />
			</AppShell.Navbar>
			<AppShell.Main bg="stone.1">
				<Stack gap={0} h="100dvh">
					<HoldBanner />
					<ScrollArea flex={1} type="hover">
						<Stack gap="lg" maw={1000} px="xl" py="lg">
							<Outlet />
						</Stack>
					</ScrollArea>
				</Stack>
				<OneOffHost />
				<ReportHost />
				<DropOverlay />
			</AppShell.Main>
		</AppShell>
	);
}
