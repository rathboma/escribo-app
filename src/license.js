// Licenses, checked on this device and nowhere else. Main process only.
//
// escribo never connects to anything, so a license can't be looked up. Instead
// each device has a device code, derived from the machine's own ID, and a
// license key is turned into an activation code for that one device on
// escriboapp.com/activate. The code is signed with escribo's Ed25519 key; the
// app checks the signature with the public key below and checks the device
// code in it against its own. A code copied to another machine names the
// wrong device, and one edited to say "business" no longer matches its
// signature.
//
// The formats are shared with the Worker behind escriboapp.com/api
// (rathboma/escribo-web, worker/src/formats.js and worker/src/activation.js),
// and test/fixtures/activation.json holds the vectors both are tested against.
const crypto = require('crypto');
const fs = require('fs');
const { execFile } = require('child_process');

const ACTIVATE_URL = 'https://escriboapp.com/activate/';

// The public halves of the keys escriboapp.com signs activation codes with,
// newest first. `npm run keygen` in escribo-web/worker makes a pair: the
// private key becomes the Worker's LICENSE_SIGNING_KEY secret and the public
// key goes here. Keep old keys in the list when rotating, or every device
// activated under them goes back to Personal.
const ACTIVATION_PUBLIC_KEYS = [
  // '-----BEGIN PUBLIC KEY-----\n…\n-----END PUBLIC KEY-----'
];

const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
const PLANS = ['pro', 'business'];

/** Bytes as Crockford base32, most significant bit first, without padding. */
function base32(bytes) {
  let out = '';
  let bits = 0;
  let value = 0;
  for (const byte of bytes) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      out += ALPHABET[(value >>> bits) & 31];
    }
    value &= (1 << bits) - 1;
  }
  if (bits > 0) out += ALPHABET[(value << (5 - bits)) & 31];
  return out;
}

function sha256(text) {
  return crypto.createHash('sha256').update(text).digest();
}

/**
 * The device code for a machine ID: 15 characters of a salted hash, so the
 * ID itself never leaves the machine, and a check character that lets the
 * website catch a mistyped code. Changing this changes every device code.
 */
function deviceCodeFromMachineId(machineId) {
  const body = base32(sha256(`escribo-device-v1:${machineId}`)).slice(0, 15);
  return body + ALPHABET[sha256(body)[0] & 31];
}

/** `XXXX-XXXX-XXXX-XXXX`, the way it is shown and typed. */
function formatDeviceCode(code) {
  return code.match(/.{1,4}/g).join('-');
}

function run(file, args) {
  return new Promise((resolve) => {
    execFile(file, args, { timeout: 5000, windowsHide: true }, (err, stdout) => resolve(err ? '' : String(stdout)));
  });
}

/**
 * The operating system's own ID for this machine: the one that stays put
 * across reboots and app updates, and only changes with a reinstall of the
 * OS. Null when it can't be read.
 */
async function readMachineId(platform = process.platform) {
  let id = null;
  if (platform === 'darwin') {
    const out = await run('/usr/sbin/ioreg', ['-rd1', '-c', 'IOPlatformExpertDevice']);
    id = (/"IOPlatformUUID"\s*=\s*"([^"]+)"/.exec(out) || [])[1];
  } else if (platform === 'win32') {
    const out = await run('reg', ['query', 'HKLM\\SOFTWARE\\Microsoft\\Cryptography', '/v', 'MachineGuid', '/reg:64']);
    id = (/MachineGuid\s+REG_SZ\s+(\S+)/.exec(out) || [])[1];
  } else {
    // Flatpak shares the host's /etc/machine-id with the sandbox.
    for (const file of ['/etc/machine-id', '/var/lib/dbus/machine-id']) {
      try {
        id = fs.readFileSync(file, 'utf8').trim();
      } catch {
        id = null;
      }
      if (id) break;
    }
  }
  return id ? id.trim().toLowerCase() : null;
}

function fromBase64url(text) {
  return Buffer.from(text.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
}

/**
 * Checks a pasted activation code against this device.
 *
 * @returns {{ ok: true, activation: { licenseId: string, plan: string, major: number, issuedAt: number } }
 *         | { ok: false, reason: 'empty' | 'license-key' | 'format' | 'no-keys' | 'signature' | 'device' | 'version' }}
 */
function verifyActivation(code, deviceCode, { publicKeys = ACTIVATION_PUBLIC_KEYS, appMajor = 1 } = {}) {
  // Pasting from an email or a PDF can bring line breaks and spaces with it.
  const compact = String(code || '').replace(/\s+/g, '');
  if (!compact) return { ok: false, reason: 'empty' };
  // Somebody pasting the license key itself, which has to go to the website first.
  if (/^ESC-?[0-9A-Z]{5}/i.test(compact)) return { ok: false, reason: 'license-key' };

  const parts = compact.split('.');
  if (parts.length !== 2 || !/^[A-Za-z0-9_-]+$/.test(parts[0]) || !/^[A-Za-z0-9_-]+$/.test(parts[1])) {
    return { ok: false, reason: 'format' };
  }
  if (!publicKeys.length) return { ok: false, reason: 'no-keys' };

  const signed = Buffer.from(parts[0]);
  const signature = fromBase64url(parts[1]);
  const valid = publicKeys.some((pem) => {
    try {
      return crypto.verify(null, signed, crypto.createPublicKey(pem), signature);
    } catch {
      return false;
    }
  });
  if (!valid) return { ok: false, reason: 'signature' };

  let payload;
  try {
    payload = JSON.parse(fromBase64url(parts[0]).toString('utf8'));
  } catch {
    return { ok: false, reason: 'format' };
  }
  if (payload.v !== 1 || !PLANS.includes(payload.plan) || typeof payload.lid !== 'string') {
    return { ok: false, reason: 'format' };
  }
  if (payload.dev !== deviceCode) return { ok: false, reason: 'device' };
  if (!(payload.major >= appMajor)) return { ok: false, reason: 'version' };

  return {
    ok: true,
    activation: { licenseId: payload.lid, plan: payload.plan, major: payload.major, issuedAt: payload.iat }
  };
}

module.exports = {
  ACTIVATE_URL,
  ACTIVATION_PUBLIC_KEYS,
  base32,
  deviceCodeFromMachineId,
  formatDeviceCode,
  readMachineId,
  verifyActivation
};
