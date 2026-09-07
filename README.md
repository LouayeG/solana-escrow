# Solana Escrow

A beginner-friendly, trust-minimized **token escrow** built with Anchor. A
*maker* offers some of token **A** in exchange for a set amount of token **B**;
a *taker* fills the offer, and the swap happens **atomically** — both legs
succeed together or the whole transaction reverts. Neither party ever has to
trust the other, because a Program Derived Address (PDA) custodies the maker's
tokens until the deal settles.

> Educational successor to [solana-token-vault](https://github.com/LouayeG/solana-token-vault):
> the vault taught one owner + one PDA; escrow adds two parties who don't trust
> each other and a PDA that settles a deal between them.

Work in progress — see the commit history for the step-by-step build.
