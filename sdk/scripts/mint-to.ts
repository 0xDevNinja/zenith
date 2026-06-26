// Mint the market's test tokens to any devnet address (the CLI wallet is the
// mint authority).  npx tsx scripts/mint-to.ts <RECIPIENT_PUBKEY> [amount]
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount, mintTo } from "@solana/spl-token";

const manifest = JSON.parse(
  readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "devnet.json"), "utf8"),
);
const payer = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8"))),
);

const recipientArg = process.argv[2];
if (!recipientArg) {
  console.error("usage: npx tsx scripts/mint-to.ts <RECIPIENT_PUBKEY> [whole-token-amount]");
  process.exit(1);
}
const recipient = new PublicKey(recipientArg);
const whole = BigInt(process.argv[3] ?? "10000");

const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");

async function main() {
  for (const [mintStr, meta] of Object.entries(manifest.mints) as [string, { symbol: string; decimals: number }][]) {
    const mint = new PublicKey(mintStr);
    const amount = whole * 10n ** BigInt(meta.decimals);
    const ata = await getOrCreateAssociatedTokenAccount(conn, payer, mint, recipient);
    await mintTo(conn, payer, mint, ata.address, payer, amount);
    console.log(`minted ${whole} ${meta.symbol} → ${recipient.toBase58()}`);
  }
  console.log("done.");
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
