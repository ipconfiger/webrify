// Wire types for the challenge/verify flow. Mirrors `turnstile_core::protocol`
// (numeric fields are `number` since realistic values fit in JS's safe-integer
// range; see the Rust crate's `#[ts(type = "number")]` annotations).

export interface Challenge {
  protocol_version: number;
  algorithm: string;
  salt: string;
  challenge: string;
  difficulty: number;
  maxnumber: number;
  expires_at: number;
  origin: string;
  signature: string;
}

export interface VerifyResponse {
  success: boolean;
  token: string;
  expires_at: number;
}
