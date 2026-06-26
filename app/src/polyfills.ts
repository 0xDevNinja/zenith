// web3.js and several wallet-adapter deps assume a Node-style Buffer global.
// This module has no other imports, so importing it first in main.tsx sets the
// global before any of those modules evaluate.
import { Buffer } from "buffer";

if (typeof globalThis.Buffer === "undefined") {
  globalThis.Buffer = Buffer;
}
