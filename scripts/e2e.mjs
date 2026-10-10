import { spawnSync } from 'node:child_process';

const run = (program, args) => {
	const result = spawnSync(program, args, { stdio: 'inherit', shell: process.platform === 'win32' });
	if (result.status !== 0) {
		process.exit(result.status ?? 1);
	}
};

run('pnpm', ['--filter', '@mivia-desktop/ui', 'build']);
run('cargo', ['build', '-p', 'mivia-desktop', '--profile', 'e2e', '--features', 'custom-protocol,e2e', '--locked']);
run('cargo', ['build', '-p', 'mivia-stub', '--profile', 'e2e', '--locked']);
run('pnpm', ['--filter', '@mivia-desktop/ui', 'e2e']);
