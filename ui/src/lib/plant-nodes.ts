// Nodes the plant has listed (pinned signer keys). A receipt is only as trustworthy as the key that signed it:
// a valid signature from a key that is NOT on this list proves that someone signed the text, not that the plant's
// GPU produced it. Same idea as `--expect-signer` / `node_id` in the gateway's nodes.json.
export const PLANT_NODES: Record<string, string> = {
  '0cb2fad76b7a53caf66892d2e7f56cb4c2d70c04929fadc50a802ba86325d17d': 'RTX 3090 · Poland',
  '10391fe96d8e2f51f050611da633cbdc9fc589aaa580f8a973d4b355dcd192e1': 'AMD Instinct MI300X · Atlanta'
};

export const plantNode = (signer?: string | null): string | null =>
  signer ? (PLANT_NODES[signer.toLowerCase()] ?? null) : null;
