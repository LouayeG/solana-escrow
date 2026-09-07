// ============================================================================
//  escrow — a trust-minimized token swap written with Anchor.
//
//  A *maker* locks some of token A in a PDA-owned vault and names a price in
//  token B. A *taker* pays that price to the maker, and the same transaction
//  releases token A to the taker. Either both legs happen or neither does, so
//  neither side has to trust the other.
//
//  Instructions (added over the following commits):
//    1. make   -> open an offer and deposit token A
//    2. take   -> fill an offer: pay token B, receive token A
//    3. cancel -> maker reclaims token A and closes the offer
// ============================================================================

use anchor_lang::prelude::*;

// Anchor's default placeholder ID. `anchor keys sync` overwrites it with the
// real program key after the first build.
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod escrow {
    use super::*;
}

// ============================================================================
//  STATE
// ============================================================================

/// One open offer, stored in a PDA seeded by the maker plus a maker-chosen
/// `seed` — so a single maker can keep several offers open at once.
#[account]
#[derive(InitSpace)]
pub struct Escrow {
    /// Maker-chosen id that makes each offer's PDA unique.
    pub seed: u64,
    /// Who opened the offer. They can cancel it, and the rent returns to them.
    pub maker: Pubkey,
    /// The token the maker deposited into the vault.
    pub mint_a: Pubkey,
    /// The token the maker wants in return.
    pub mint_b: Pubkey,
    /// How much of `mint_b` the taker must pay. The deposited amount of
    /// `mint_a` is deliberately NOT stored here — the vault token account holds
    /// it, and that balance is the single source of truth (a lesson carried
    /// over from the token vault).
    pub receive: u64,
    /// Canonical bump for the escrow PDA.
    pub bump: u8,
}

// ============================================================================
//  ERRORS
// ============================================================================

#[error_code]
pub enum EscrowError {
    #[msg("Amounts must be greater than zero")]
    ZeroAmount,
    #[msg("Token account is for a different mint")]
    WrongMint,
    #[msg("Token account belongs to someone else")]
    WrongOwner,
}
