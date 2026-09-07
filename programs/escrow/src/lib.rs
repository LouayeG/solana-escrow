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
