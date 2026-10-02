import * as ed from '@noble/ed25519';
import { sha256 } from '@noble/hashes/sha256';
import { sha512 } from '@noble/hashes/sha512';
import { bytesToHex } from '@noble/hashes/utils';
import { decodeHex, type Receipt } from './receipt';

// Synchronous Ed25519 (no WebCrypto needed, so it works on file:// and offline).
ed.etc.sha512Sync = (...m) => sha512(ed.etc.concatBytes(...m));

export interface VerifyDetail {
  signatureOk: boolean;
  recomputedHash: string;
  hashesMatch: boolean;
  valid: boolean;
}

export function verifyDetailed(r: Receipt): VerifyDetail {
  const recomputedHash = bytesToHex(sha256(new TextEncoder().encode(r.output)));
  const hashesMatch = recomputedHash === r.output_sha256.toLowerCase();
  let signatureOk = false;
  try {
    signatureOk = ed.verify(decodeHex(r.signature, 'ed25519_sig'), decodeHex(r.output_sha256), decodeHex(r.node_pubkey, 'ed25519'));
  } catch {
    signatureOk = false;
  }
  return { signatureOk, recomputedHash, hashesMatch, valid: hashesMatch && signatureOk };
}

/** True only if sha256(output) equals the signed hash AND the signature over that hash is valid. */
export function verifyReceipt(r: Receipt): boolean {
  return verifyDetailed(r).valid;
}
