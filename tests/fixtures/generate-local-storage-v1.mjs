// Offline fixture generator. Public test seed only; no agents or key-file IO.
// stdout is the complete key-file fixture; stderr contains intermediate vectors.
import {
    createCipheriv,
    createHash,
    createPrivateKey,
    createPublicKey,
    hkdfSync,
    sign,
} from 'node:crypto';

const u32 = (value) => {
    const bytes = Buffer.alloc(4);
    bytes.writeUInt32BE(value);
    return bytes;
};
const string = (bytes) => Buffer.concat([u32(bytes.length), bytes]);
const privateKey = createPrivateKey({
    key: Buffer.concat([
        Buffer.from('302e020100300506032b657004220420', 'hex'),
        Buffer.alloc(32, 7),
    ]),
    format: 'der',
    type: 'pkcs8',
});
const raw = createPublicKey(privateKey).export({ format: 'der', type: 'spki' }).subarray(-32);
const blob = Buffer.concat([string(Buffer.from('ssh-ed25519')), string(raw)]);
const salt = Buffer.alloc(32, 1);
const nonce = Buffer.alloc(12, 2);
const id = 'fixture';
const aad = Buffer.concat([
    Buffer.from('psst/local-storage/aad\0'),
    u32(1),
    string(blob),
    string(Buffer.from(id)),
    salt,
    nonce,
]);
const challenge = Buffer.concat([Buffer.from('psst/local-storage/sign\0'), aad]);
const signature = sign(null, challenge, privateKey);
const info = Buffer.concat([Buffer.from('psst/local-storage/hkdf\0'), aad]);
const key = Buffer.from(hkdfSync('sha256', signature, salt, info, 32));
const cipher = createCipheriv('aes-256-gcm', key, nonce);
cipher.setAAD(aad);
const ciphertext = Buffer.concat([
    cipher.update(' \n🔑\t'),
    cipher.final(),
    cipher.getAuthTag(),
]);
const hash = createHash('sha256').update(blob).digest();
console.log(JSON.stringify({
    version: 1,
    public_key: blob.toString('base64'),
    fingerprint: `SHA256:${hash.toString('base64').replace(/=+$/, '')}`,
    records: [{
        version: 1,
        id,
        public_key: blob.toString('base64'),
        salt: salt.toString('base64'),
        nonce: nonce.toString('base64'),
        ciphertext: ciphertext.toString('base64'),
    }],
}, null, 2));
for (const [name, bytes] of Object.entries({ aad, challenge, signature, key })) {
    console.error(`${name}: ${bytes.toString('hex')}`);
}
