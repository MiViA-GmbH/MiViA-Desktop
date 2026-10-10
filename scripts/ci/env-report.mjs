import { spawnSync } from 'node:child_process';

const version = (program, args) => {
	const result = spawnSync(program, args, { encoding: 'utf8' });
	return result.status === 0 ? result.stdout.trim() : `${program} not available`;
};

const lines = [
	`image: ${process.env.ImageOS ?? 'local'} ${process.env.ImageVersion ?? ''}`.trim(),
	`node: ${process.version}`,
	`rustc: ${version('rustc', ['-V'])}`,
	`rustup: ${version('rustup', ['--version'])}`,
];
process.stdout.write(`${lines.join('\n')}\n`);
