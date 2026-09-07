# Understanding the escrow, step by step

Read this with `programs/escrow/src/lib.rs` open beside you. If you built the
[token vault](https://github.com/LouayeG/solana-token-vault) first, most of this
is familiar — the new idea is **one PDA settling a trade between two parties who
don't trust each other**.

---

## 0. The mental model

An escrow is a promise the chain enforces:

> "I (the maker) will give you `deposit` of token A **if** you give me `receive`
> of token B — and the swap happens in a single transaction, so it's impossible
> for one side to take without giving."

Three accounts carry the state:

- **`escrow`** — a PDA that stores the *terms* (maker, both mints, the price).
- **`vault`** — a token account **owned by the escrow PDA** that physically
  holds the maker's token A. Because a PDA has no private key, only this program
  can move those tokens.
- the maker's and taker's own token accounts.

The whole design rests on one fact from the vault project: **a PDA can own
tokens, and only its program can sign to release them.**

---

## 1. Per-offer PDAs

```rust
seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()]
```

The vault used `["vault", owner, mint]` — one vault per owner per mint. Here the
maker also supplies a `seed` (any `u64`), so the *same* maker can run many offers
at once, each at its own address. Store the canonical `bump` so later
instructions don't re-derive it.

---

## 2. `make` — lock the tokens, record the deal

```rust
ctx.accounts.escrow.set_inner(Escrow { seed, maker, mint_a, mint_b, receive, bump });
token::transfer(cpi_ctx, deposit)?; // maker -> vault, maker signs
```

Two things happen: the terms are written into the escrow account, and the
maker's token A moves into the vault. The maker signed the transaction, so
moving *their own* tokens is a plain CPI — no PDA signing yet.

Notice what is **not** stored: the deposited amount. The vault's balance is the
source of truth, exactly like the vault project. One less field to desync.

---

## 3. `cancel` — the maker backs out

This is the vault's `close_vault` with a new label. The escrow PDA signs to move
token A back to the maker, closes the vault, and Anchor closes the escrow
account (`close = maker`). Rent flows home. Nothing here you haven't seen.

---

## 4. `take` — the part that's actually new

```rust
// Leg 1: taker -> maker, token B (taker signs)
token::transfer(cpi_b, ctx.accounts.escrow.receive)?;

// Leg 2: vault -> taker, token A (escrow PDA signs)
let signer_seeds = &[&[b"escrow", maker_key.as_ref(), seed_bytes.as_ref(), &[bump]]];
token::transfer(cpi_a_with_signer, amount)?;
```

Both transfers are in **one instruction, one transaction**. Solana transactions
are atomic: if leg 2 failed, leg 1 is rolled back too. That is the entire reason
an escrow is safe — the taker cannot pay and receive nothing, and the vault
cannot release without payment. There is no moment in between for either party
to cheat.

Leg 1 is signed by the taker (their own token B). Leg 2 is signed by the escrow
**PDA** (the vault's authority) using the same seeds as in `make`, plus the bump.

---

## 5. The constraints are the security model

The dangerous instruction is `take`, because a *stranger* calls it. What stops a
malicious taker from redirecting the payout?

```rust
#[account(mut, close = maker, has_one = maker, has_one = mint_a, has_one = mint_b,
          seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
          bump = escrow.bump)]
pub escrow: Account<'info, Escrow>,
```

- `has_one = maker` / `mint_a` / `mint_b` — the accounts passed in must equal the
  ones the maker committed to when opening the offer. The taker can't swap in
  their own `maker` account to steal the deposit, or a different mint to pay in a
  worthless token.
- `seeds` + `bump` — this really is the offer at that address.
- The maker's payout account is an ATA derived from `maker` + `mint_b`, so it
  can only ever belong to the real maker.

Missing account checks are the #1 source of Solana exploits. Anchor's value is
that all of them sit in one visible place instead of being scattered or
forgotten.

---

## 6. What to try next

1. **Partial fills** — let a taker fill *part* of an offer and leave the rest
   open. (Now the deposited amount matters relative to `receive` — think about
   rounding.)
2. **Expiry** — store `deadline: i64` and reject `take` after
   `Clock::get()?.unix_timestamp`.
3. **A fee** — skim a few basis points to a treasury on each `take`.
4. **Token-2022** — run an offer where token B has a transfer fee, and watch how
   the maker receives *less* than `receive`. This is exactly the kind of trap
   worth recognizing in the wild.

---

## Glossary

| Term | Meaning |
|---|---|
| **PDA** | Program Derived Address — an address off the ed25519 curve, so only its program can sign for it |
| **CPI** | Cross-Program Invocation — one program calling another mid-transaction |
| **ATA** | Associated Token Account — the canonical token account for a (wallet, mint) pair, at a deterministic address |
| **Atomic** | All of a transaction's instructions succeed together, or none of them do |
| **Vault** | Here: the token account owned by the escrow PDA that holds the maker's deposit |
| **Event** | A structured, decodable log a program emits for off-chain indexers |
