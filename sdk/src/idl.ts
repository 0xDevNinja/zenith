import idlJson from "../idl/zenith_amm.json" with { type: "json" };

/// The committed zenith-amm IDL: program address, PDA seeds, instruction arg
/// layouts, and account field layouts. The single descriptor the rest of the
/// SDK (account decoders, transaction builders) is generated/typed against.
export const ZENITH_AMM_IDL = idlJson;
export type ZenithAmmIdl = typeof idlJson;

/// Instruction names exposed by the program (from the IDL).
export type ZenithInstructionName = ZenithAmmIdl["instructions"][number]["name"];

/// Account names defined by the program (from the IDL).
export type ZenithAccountName = ZenithAmmIdl["accounts"][number]["name"];
