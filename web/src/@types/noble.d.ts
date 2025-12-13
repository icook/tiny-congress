/**
 * Type declarations for @noble packages
 */

declare module '@noble/curves/ed25519' {
  export const ed25519: {
    sign: (message: Uint8Array, privateKey: Uint8Array) => Uint8Array;
    verify: (signature: Uint8Array, message: Uint8Array, publicKey: Uint8Array) => boolean;
    getPublicKey: (privateKey: Uint8Array) => Uint8Array;
    utils: {
      randomPrivateKey: () => Uint8Array;
    };
  };
}

declare module '@noble/hashes/sha256' {
  export function sha256(data: Uint8Array): Uint8Array;
}
