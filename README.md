# Solana Escrow

A beginner-friendly, trust-minimized **token escrow** built with Anchor. A
*maker* offers some of token **A** in exchange for a set amount of token **B**; a
*taker* fills the offer, and the swap settles **atomically** — both legs succeed
together or the whole transaction reverts. Neither party has to trust the other,
because a Program Derived Address (PDA) custodies the maker's tokens until the
deal closes.

> Educational successor to [solana-token-vault](https://github.com/LouayeG/solana-token-vault):
> the vault taught one owner + one PDA; escrow adds two parties who don't trust
> each other and a PDA that settles a deal between them.

## How it works

```text
make:   maker's token A ──▶ vault (owned by the escrow PDA)   terms stored in escrow
take:   taker's token B ──▶ maker            (leg 1, taker signs)
        vault's token A ──▶ taker            (leg 2, escrow PDA signs)
        ── both legs in ONE transaction: all-or-nothing ──
cancel: vault's token A ──▶ maker            then escrow + vault closed (rent → maker)
```

The **escrow PDA** is seeded by `["escrow", maker, seed]`, so one maker can keep
several offers open at once. It stores the terms (maker, both mints, the price in
token B). The **vault** is an associated token account owned by that PDA; its
balance is the single source of truth for the deposit (no shadow counter).

## Instructions

| Instruction | Behavior |
| --- | --- |
| `make(seed, deposit, receive)` | Opens an offer: records the terms and moves `deposit` of token A into the vault. The maker signs. |
| `take()` | Fills an offer atomically: the taker pays `receive` of token B to the maker, the escrow PDA releases token A to the taker, then the vault and escrow are closed. |
| `cancel()` | The maker reclaims their token A and closes the offer, refunding all rent. |

Each instruction emits an event (`OfferMade` / `OfferTaken` / `OfferCancelled`)
for off-chain indexers.

## Prerequisites

- Rust and Cargo
- Solana CLI
- Anchor CLI `0.31.1`
- Node.js 18 or newer and npm

On Windows, run the toolchain inside WSL. See the official
[Solana](https://solana.com/docs/intro/installation) and
[Anchor](https://www.anchor-lang.com/docs/installation) install guides.

## Run locally

```bash
npm install
anchor build
anchor keys sync   # writes the real program id into Anchor.toml + declare_id!
anchor build
anchor test
```

The integration suite covers `make`, `take`, and `cancel`, plus a zero-amount
rejection and an unauthorized-cancel attempt.

## Project structure

```text
.
|-- Anchor.toml                    Anchor workspace and localnet settings
|-- Cargo.toml                     Rust workspace configuration
|-- programs/escrow/
|   |-- Cargo.toml                 On-chain program crate
|   `-- src/lib.rs                 Instructions, accounts, state, events, errors
|-- tests/escrow.ts                Local-validator integration tests
|-- EXPLAINER.md                   Step-by-step walkthrough of the design
|-- package.json                   JavaScript tooling and dependencies
`-- tsconfig.json                  TypeScript test configuration
```

For a deeper explanation of PDAs, CPI signing, atomicity, and every account
constraint, read [EXPLAINER.md](EXPLAINER.md).

## Security notes

- The swap is atomic: `take` moves both legs in one transaction, so no party can
  take without giving.
- `has_one` on the maker and both mints pins every `take`/`cancel` to the exact
  terms the maker committed to — a taker can't redirect the payout or pay in the
  wrong token.
- The vault is owned by the escrow PDA; only this program can release it.
- The vault's token balance is the single source of truth (no separate counter
  to drift out of sync).
- This is an educational program — no partial fills, expiry, oracle, fee, or
  production audit.

## License

Licensed under the [MIT License](LICENSE).
