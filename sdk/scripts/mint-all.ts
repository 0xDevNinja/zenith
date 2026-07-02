// Mint every engine's test tokens to a recipient (CLI wallet is mint authority).
//   npx tsx scripts/mint-all.ts <RECIPIENT> [whole-amount]
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";

const recipient = new PublicKey(process.argv[2]);
const whole = BigInt(process.argv[3] ?? "10000");
const payer = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8"))),
);
const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
const dir = join(import.meta.dirname, "..", "..", "app", "src");
const manifests = ["devnet.json", "dlmm-devnet.json", "camm-devnet.json"];

async function main() {
  const seen = new Set<string>();
  for (const mf of manifests) {
    const m = JSON.parse(readFileSync(join(dir, mf), "utf8"));
    for (const [mintStr, meta] of Object.entries(m.mints) as [string, { symbol: string; decimals: number }][]) {
      if (seen.has(mintStr)) continue;
      seen.add(mintStr);
      const mint = new PublicKey(mintStr);
      const amount = whole * 10n ** BigInt(meta.decimals);
      const ata = await getOrCreateAssociatedTokenAccount(conn, payer, mint, recipient);
      await mintTo(conn, payer, mint, ata.address, payer, amount);
      console.log(`minted ${whole} ${meta.symbol} (${mintStr}) -> ${recipient.toBase58()}`);
    }
  }
  console.log("DONE");
}
main().catch((e) => { console.error(e); process.exit(1); });
