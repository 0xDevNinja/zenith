// Turn the grab-bag of wallet / RPC / program errors into one human sentence.
// Anchor programs emit "Error Message: <text>" in the logs, so we surface that
// verbatim when present; otherwise we pattern-match the common failure modes.
export function decodeTxError(err: unknown): string {
  const e = err as { message?: string; logs?: string[] } | undefined;
  const msg = String(e?.message ?? err ?? "");

  // User dismissed the wallet prompt.
  if (/reject|denied|declined|cancel|user.*rejected/i.test(msg)) {
    return "Transaction rejected in your wallet.";
  }
  // Blockhash aged out before it landed.
  if (/block ?height exceeded|blockhash not found|transaction expired|expired/i.test(msg)) {
    return "Transaction expired before confirming — please try again.";
  }
  // Not enough SOL for fees / rent.
  if (/insufficient.*(lamports|funds)|debit an account|0x1\b.*lamport/i.test(msg)) {
    return "Not enough SOL to cover network fees.";
  }
  // Anchor's human message from the program logs.
  const logs = e?.logs ?? [];
  const anchorMsg = logs.find((l) => l.includes("Error Message:"));
  if (anchorMsg) return anchorMsg.split("Error Message:")[1].trim();

  // A raw custom program error code (no decoded message available).
  const custom = msg.match(/custom program error: (0x[0-9a-fA-F]+)/);
  if (custom) {
    if (/0x1771|slippage|threshold/i.test(msg)) return "Price moved past your slippage — try again.";
    return `Program rejected the transaction (${custom[1]}).`;
  }

  // Network / RPC trouble.
  if (/fetch|network|timeout|429|rate.?limit/i.test(msg)) {
    return "Network error talking to devnet — please retry.";
  }

  const firstLine = msg.split("\n")[0].trim();
  return firstLine ? firstLine.slice(0, 160) : "Transaction failed.";
}
