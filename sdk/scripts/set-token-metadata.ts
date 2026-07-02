// Attach Metaplex token metadata (name + symbol) to each engine's test mints so
// wallets show real names instead of "Unknown Token". CLI wallet is the mint
// authority, so it can create the metadata account.
//   npx tsx scripts/set-token-metadata.ts
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import {
  createMetadataAccountV3,
  mplTokenMetadata,
  fetchMetadataFromSeeds,
} from "@metaplex-foundation/mpl-token-metadata";
import {
  createSignerFromKeypair,
  signerIdentity,
  publicKey as umiPk,
} from "@metaplex-foundation/umi";

const RPC = process.env.RPC ?? "https://api.devnet.solana.com";
const secret = Uint8Array.from(
  JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8")),
);

// mint -> friendly name (symbol comes from the manifests).
const NAMES: Record<string, string> = {};
const dir = join(import.meta.dirname, "..", "..", "app", "src");
const label: Record<string, string> = {
  "devnet.json": "",
  "dlmm-devnet.json": " (DLMM)",
  "camm-devnet.json": " (Yield)",
};
const meta: Record<string, { symbol: string; decimals: number }> = {};
for (const [mf, suffix] of Object.entries(label)) {
  const m = JSON.parse(readFileSync(join(dir, mf), "utf8"));
  for (const [mint, info] of Object.entries(m.mints) as [string, { symbol: string; decimals: number }][]) {
    if (meta[mint]) continue;
    meta[mint] = info;
    NAMES[mint] = `Zenith ${info.symbol}${suffix}`;
  }
}

async function main() {
  const umi = createUmi(RPC).use(mplTokenMetadata());
  const kp = umi.eddsa.createKeypairFromSecretKey(secret);
  umi.use(signerIdentity(createSignerFromKeypair(umi, kp)));

  for (const [mint, info] of Object.entries(meta)) {
    const m = umiPk(mint);
    try {
      await fetchMetadataFromSeeds(umi, { mint: m });
      console.log(`skip ${info.symbol} — metadata already exists`);
      continue;
    } catch {
      /* none yet */
    }
    await createMetadataAccountV3(umi, {
      mint: m,
      mintAuthority: umi.identity,
      payer: umi.identity,
      updateAuthority: umi.identity,
      data: {
        name: NAMES[mint],
        symbol: info.symbol,
        uri: "",
        sellerFeeBasisPoints: 0,
        creators: null,
        collection: null,
        uses: null,
      },
      isMutable: true,
      collectionDetails: null,
    }).sendAndConfirm(umi);
    console.log(`set "${NAMES[mint]}" (${info.symbol}) on ${mint}`);
  }
  console.log("DONE");
}
main().catch((e) => { console.error(e); process.exit(1); });
