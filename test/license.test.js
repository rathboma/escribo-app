// The app's half of licensing, against the vectors escribo-web's Worker is
// tested with (test/fixtures/activation.json, copied from its
// worker/test/fixtures). Run with `yarn test`.
const test = require('node:test');
const assert = require('node:assert/strict');
const fixture = require('./fixtures/activation.json');
const { base32, deviceCodeFromMachineId, formatDeviceCode, readMachineId, verifyActivation } = require('../src/license.js');

const [HERE, THERE] = fixture.devices;
const keys = { publicKeys: [fixture.publicKey] };
const { code } = fixture.activation;

function recode(payload, signature = code.split('.')[1]) {
  return `${Buffer.from(JSON.stringify(payload)).toString('base64url')}.${signature}`;
}

test('base32 matches the Worker', () => {
  assert.equal(base32(Buffer.from([0xff])), 'ZW');
  assert.equal(base32(Buffer.from('foobar')), 'CSQPYRK1E8');
});

test('device codes match the vectors the Worker checks', () => {
  for (const { machineId, deviceCode } of fixture.devices) {
    assert.equal(deviceCodeFromMachineId(machineId), deviceCode);
  }
  assert.equal(formatDeviceCode(HERE.deviceCode), 'V14Y-SQP1-WW7G-HAXX');
});

test('a code signed for this device activates it', () => {
  assert.deepEqual(verifyActivation(code, HERE.deviceCode, keys), {
    ok: true,
    activation: { licenseId: 'lic_fixture0000000001', plan: 'pro', major: 1, issuedAt: 1790000000 }
  });
});

test('line breaks and spaces from pasting are fine', () => {
  const pasted = `  ${code.slice(0, 40)}\n${code.slice(40, 90)} \r\n${code.slice(90)}\n`;
  assert.equal(verifyActivation(pasted, HERE.deviceCode, keys).ok, true);
});

test("another device's code doesn't activate this one", () => {
  assert.deepEqual(verifyActivation(code, THERE.deviceCode, keys), { ok: false, reason: 'device' });
});

test('an edited code no longer matches its signature', () => {
  const upgraded = recode({ ...fixture.activation.payload, plan: 'business' });
  assert.deepEqual(verifyActivation(upgraded, HERE.deviceCode, keys), { ok: false, reason: 'signature' });
  const moved = recode({ ...fixture.activation.payload, dev: THERE.deviceCode });
  assert.deepEqual(verifyActivation(moved, THERE.deviceCode, keys), { ok: false, reason: 'signature' });
});

test("a code signed with somebody else's key is refused", () => {
  const { privateKey, publicKey } = require('crypto').generateKeyPairSync('ed25519');
  const payload = Buffer.from(JSON.stringify(fixture.activation.payload)).toString('base64url');
  const forged = `${payload}.${require('crypto').sign(null, Buffer.from(payload), privateKey).toString('base64url')}`;
  assert.deepEqual(verifyActivation(forged, HERE.deviceCode, keys), { ok: false, reason: 'signature' });
  // ...and it would pass with that key, so the refusal is about the key.
  const theirs = publicKey.export({ type: 'spki', format: 'pem' });
  assert.equal(verifyActivation(forged, HERE.deviceCode, { publicKeys: [theirs] }).ok, true);
});

test('any of several public keys will do, for rotating keys', () => {
  const { publicKey } = require('crypto').generateKeyPairSync('ed25519');
  const newer = publicKey.export({ type: 'spki', format: 'pem' });
  assert.equal(verifyActivation(code, HERE.deviceCode, { publicKeys: [newer, fixture.publicKey] }).ok, true);
});

test('a license for 1.x does not activate 2.x', () => {
  assert.deepEqual(verifyActivation(code, HERE.deviceCode, { ...keys, appMajor: 2 }), { ok: false, reason: 'version' });
});

test('says why anything else is refused', () => {
  assert.deepEqual(verifyActivation('', HERE.deviceCode, keys), { ok: false, reason: 'empty' });
  assert.deepEqual(verifyActivation('ESC-K7Q2M-9XJ4T-A1B2C-3D4E5', HERE.deviceCode, keys), { ok: false, reason: 'license-key' });
  assert.deepEqual(verifyActivation('not.a code!', HERE.deviceCode, keys), { ok: false, reason: 'format' });
  assert.deepEqual(verifyActivation('abc', HERE.deviceCode, keys), { ok: false, reason: 'format' });
  assert.deepEqual(verifyActivation(code, HERE.deviceCode, { publicKeys: [] }), { ok: false, reason: 'no-keys' });
});

test('reads a machine ID on this OS', async () => {
  const id = await readMachineId();
  // CI containers don't always have one; when there is one it is lower-case.
  if (id !== null) assert.equal(id, id.toLowerCase());
});
