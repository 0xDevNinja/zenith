import { clusterApiUrl, type Commitment } from "@solana/web3.js";
import { ZENITH_AMM_PROGRAM_ID } from "@zenith/sdk";

// Devnet only. Override the RPC with VITE_RPC_ENDPOINT (e.g. a Helius/Triton
// devnet URL) for higher rate limits; falls back to the public devnet cluster.
export const RPC_ENDPOINT: string =
  import.meta.env.VITE_RPC_ENDPOINT ?? clusterApiUrl("devnet");

export const COMMITMENT: Commitment = "confirmed";

export const NETWORK_LABEL = "Devnet";

export const PROGRAM_ID = ZENITH_AMM_PROGRAM_ID;

export const EXPLORER_BASE = "https://explorer.solana.com";

export function explorerAddress(address: string): string {
  return `${EXPLORER_BASE}/address/${address}?cluster=devnet`;
}

export function explorerTx(signature: string): string {
  return `${EXPLORER_BASE}/tx/${signature}?cluster=devnet`;
}
