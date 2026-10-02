export interface Receipt {
  job_id: string;
  node_pubkey: string;
  model: string;
  prompt_sha256: string;
  output: string;
  output_sha256: string;
  signature: string;
}

const FIELDS: (keyof Receipt)[] = ['job_id', 'node_pubkey', 'model', 'prompt_sha256', 'output', 'output_sha256', 'signature'];

/** Parse and shape-check a receipt from JSON text or an object. Throws on malformed input. */
export function parseReceipt(input: unknown): Receipt {
  const raw = typeof input === 'string' ? JSON.parse(input) : input;
  if (typeof raw !== 'object' || raw === null) throw new Error('Receipt must be a JSON object');
  const out = {} as Record<string, string>;
  for (const f of FIELDS) {
    const v = (raw as Record<string, unknown>)[f];
    if (typeof v !== 'string') throw new Error(`Receipt field "${f}" must be a string`);
    out[f] = v;
  }
  return out as unknown as Receipt;
}

/** Strip a "prefix:" label and decode hex. Throws on invalid hex. */
export function decodeHex(value: string, prefix?: string): Uint8Array {
  let hex = value;
  if (prefix && hex.startsWith(prefix + ':')) hex = hex.slice(prefix.length + 1);
  if (hex.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(hex)) throw new Error('invalid hex');
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}
