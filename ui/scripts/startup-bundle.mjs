import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dist = new URL('../dist/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const html = readFileSync(join(dist, 'index.html'), 'utf8');
const files = [...html.matchAll(/(?:src|href)="\/(assets\/[^"]+\.js)"/g)].map((match) => match[1]);
if (files.length === 0) {
	process.stderr.write('no startup scripts found in dist/index.html\n');
	process.exit(1);
}
writeFileSync(join(dist, 'startup.bundle.js'), files.map((file) => readFileSync(join(dist, file))).join('\n'));
process.stdout.write(`startup scripts: ${files.length}\n`);
