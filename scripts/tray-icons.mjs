import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { Resvg } from '@resvg/resvg-js';

const sizes = [16, 20, 24, 32, 40, 48];
const source = join('src-tauri', 'icons', 'tray', 'svg');
const target = join('src-tauri', 'icons', 'tray');

mkdirSync(target, { recursive: true });
for (const file of readdirSync(source).filter((name) => name.endsWith('.svg'))) {
	const svg = readFileSync(join(source, file));
	const name = file.replace(/^tray-/, '').replace(/\.svg$/, '');
	for (const size of sizes) {
		const png = new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
		writeFileSync(join(target, `${name}-${size}.png`), png);
	}
}
