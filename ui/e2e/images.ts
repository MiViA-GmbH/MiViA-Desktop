import { crc32, deflateSync } from 'node:zlib';

export function jpeg(seed: number): Buffer {
	return Buffer.from([0xff, 0xd8, 0xff, 0xe0, seed, 0x00, 0x10, seed, 0xff, 0xd9]);
}

export function png(width: number, height: number): Buffer {
	const raw = Buffer.alloc(height * (1 + width * 3));
	for (let y = 0; y < height; y += 1) {
		for (let x = 0; x < width; x += 1) {
			raw.set([Math.floor((x * 255) / width), Math.floor((y * 255) / height), 96], y * (1 + width * 3) + 1 + x * 3);
		}
	}
	const chunk = (type: string, body: Buffer) => {
		const named = Buffer.concat([Buffer.from(type), body]);
		const length = Buffer.alloc(4);
		length.writeUInt32BE(body.length);
		const check = Buffer.alloc(4);
		check.writeUInt32BE(crc32(named));
		return Buffer.concat([length, named, check]);
	};
	const header = Buffer.alloc(13);
	header.writeUInt32BE(width, 0);
	header.writeUInt32BE(height, 4);
	header.set([8, 2, 0, 0, 0], 8);
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', header),
		chunk('IDAT', deflateSync(raw)),
		chunk('IEND', Buffer.alloc(0)),
	]);
}
