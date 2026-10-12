import { execFileSync } from 'node:child_process';

const repo = process.env.GITHUB_REPOSITORY ?? '';
const override = (process.env.OVERRIDE ?? '').replace(/[\r\n]/g, ' ').trim();

const open = JSON.parse(
	execFileSync(
		'gh',
		['issue', 'list', '--repo', repo, '--label', 'perf-red', '--state', 'open', '--json', 'number,title'],
		{ encoding: 'utf8' },
	),
);

for (const issue of open) {
	process.stdout.write(`perf-gate: open perf-red #${issue.number} ${issue.title}\n`);
}
if (open.length === 0) {
	process.stdout.write(`perf-gate: no open perf-red issue for ${process.env.TAG}\n`);
} else if (override === '') {
	process.stderr.write('perf-gate: open perf-red issues block the release, pass perf_override_reason to override\n');
	process.exit(1);
} else {
	process.stdout.write(`::warning::perf gate overridden: ${override}\n`);
}
