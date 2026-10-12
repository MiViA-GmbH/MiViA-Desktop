import { createHash, createPublicKey, verify } from 'node:crypto';
import { readFileSync } from 'node:fs';

const decode = (text) => Buffer.from(text.trim(), 'base64');

const lines = (base64) =>
	decode(base64)
		.toString('utf8')
		.split('\n')
		.map((line) => line.trim());

export const productionKeys = () => {
	const source = readFileSync('crates/adapters/update-sig/src/lib.rs', 'utf8');
	const production = source.split('#[cfg(feature = "test-keys")]')[0];
	return [...production.matchAll(/pub const KEY_[AB]: &str = "([^"]+)";/g)].map((match) => match[1]);
};

const publicKey = (base64) => {
	const raw = decode(lines(base64)[1]);
	if (raw.length !== 42 || raw.subarray(0, 2).toString() !== 'Ed') {
		throw new Error('not a minisign public key');
	}
	const x = raw.subarray(10).toString('base64url');
	return { id: raw.subarray(2, 10), key: createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x }, format: 'jwk' }) };
};

const signedVersion = (comment) =>
	comment
		.split('\t')
		.find((field) => field.startsWith('version:'))
		?.slice('version:'.length)
		.replace(/^v/, '');

const checkWith = (keyBase64, data, signatureBase64, version) => {
	const { id, key } = publicKey(keyBase64);
	const [, signatureLine, commentLine, globalLine] = lines(signatureBase64);
	const raw = decode(signatureLine);
	const algorithm = raw.subarray(0, 2).toString();
	if (raw.length !== 74 || !['Ed', 'ED'].includes(algorithm) || !raw.subarray(2, 10).equals(id)) {
		return 'signature';
	}
	const message = algorithm === 'ED' ? createHash('blake2b512').update(data).digest() : data;
	const comment = commentLine.replace(/^trusted comment: /, '');
	const body = verify(null, message, key, raw.subarray(10));
	const global = verify(null, Buffer.concat([raw.subarray(10), Buffer.from(comment)]), key, decode(globalLine));
	if (!(body && global)) {
		return 'signature';
	}
	return signedVersion(comment) === version.replace(/^v/, '') ? 'ok' : 'signed version';
};

export const checkInstaller = (keys, data, signatureBase64, version) => {
	const results = keys.map((key) => {
		try {
			return checkWith(key, data, signatureBase64, version);
		} catch {
			return 'signature';
		}
	});
	const slot = results.indexOf('ok');
	if (slot >= 0) {
		return ['A', 'B'][slot];
	}
	throw new Error(results.includes('signed version') ? 'signed version does not match' : 'bad signature');
};
