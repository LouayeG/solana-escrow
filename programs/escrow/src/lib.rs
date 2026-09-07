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
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, CloseAccount, Mint, Token, TokenAccount, Transfer},
};

// Anchor's default placeholder ID. `anchor keys sync` overwrites it with the
// real program key after the first build.
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod escrow {
    use super::*;

    /// Open an offer: record its terms and move `deposit` units of token A from
    /// the maker into a PDA-owned vault. The maker signs the transaction, so
    /// moving their own tokens is a plain CPI — no PDA signing needed here.
    pub fn make(ctx: Context<Make>, seed: u64, deposit: u64, receive: u64) -> Result<()> {
        require!(deposit > 0 && receive > 0, EscrowError::ZeroAmount);

        // Record the terms of the deal in the escrow account.
        ctx.accounts.escrow.set_inner(Escrow {
            seed,
            maker: ctx.accounts.maker.key(),
            mint_a: ctx.accounts.mint_a.key(),
            mint_b: ctx.accounts.mint_b.key(),
            receive,
            bump: ctx.bumps.escrow,
        });

        // Lock the maker's token A in the vault.
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.maker_ata_a.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.maker.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, deposit)?;

        msg!("Offer {} opened: deposit locked, wants {} of mint_b", seed, receive);
        Ok(())
    }

    /// Cancel an open offer: the maker reclaims their token A, then the vault
    /// and escrow accounts are closed with their rent refunded to the maker.
    /// The vault is owned by the escrow PDA, so the program signs for it.
    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        // Rebuild the escrow PDA's seeds so it can sign the transfer and close.
        let maker_key = ctx.accounts.maker.key();
        let seed_bytes = ctx.accounts.escrow.seed.to_le_bytes();
        let bump = ctx.accounts.escrow.bump;
        let signer_seeds: &[&[&[u8]]] =
            &[&[b"escrow", maker_key.as_ref(), seed_bytes.as_ref(), &[bump]]];

        // Return every token A held in the vault to the maker.
        let amount = ctx.accounts.vault.amount;
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.maker_ata_a.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info(),
            },
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;

        // Close the now-empty vault; its rent goes back to the maker. The
        // escrow account is closed by Anchor via `close = maker`.
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.vault.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info(),
            },
            signer_seeds,
        );
        token::close_account(cpi_ctx)?;

        msg!("Offer {} cancelled; deposit returned", ctx.accounts.escrow.seed);
        Ok(())
    }

    /// Fill an offer. The taker pays the maker `receive` units of token B, and
    /// the same transaction releases the vault's token A to the taker. Both legs
    /// settle together or the whole transaction reverts — that atomicity is the
    /// entire reason neither party has to trust the other.
    pub fn take(ctx: Context<Take>) -> Result<()> {
        // Leg 1: taker -> maker, in token B. The taker signs for their own funds.
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.taker_ata_b.to_account_info(),
                to: ctx.accounts.maker_ata_b.to_account_info(),
                authority: ctx.accounts.taker.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, ctx.accounts.escrow.receive)?;

        // Leg 2: vault -> taker, in token A. The vault is owned by the escrow
        // PDA, so the program signs with the escrow's seeds.
        let maker_key = ctx.accounts.maker.key();
        let seed_bytes = ctx.accounts.escrow.seed.to_le_bytes();
        let bump = ctx.accounts.escrow.bump;
        let signer_seeds: &[&[&[u8]]] =
            &[&[b"escrow", maker_key.as_ref(), seed_bytes.as_ref(), &[bump]]];

        let amount = ctx.accounts.vault.amount;
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.taker_ata_a.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info(),
            },
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;

        // Close the emptied vault (rent to the maker); Anchor closes the escrow.
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.vault.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info(),
            },
            signer_seeds,
        );
        token::close_account(cpi_ctx)?;

        msg!("Offer {} filled", ctx.accounts.escrow.seed);
        Ok(())
    }
}

// ============================================================================
//  ACCOUNT CONTEXTS
// ============================================================================

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Make<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub mint_a: Account<'info, Mint>,
    pub mint_b: Account<'info, Mint>,

    /// The offer account. Seeded by the maker and their chosen `seed`, so the
    /// same maker can run multiple offers in parallel.
    #[account(
        init,
        payer = maker,
        space = 8 + Escrow::INIT_SPACE,
        seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump
    )]
    pub escrow: Account<'info, Escrow>,

    /// Holds the maker's token A until the deal settles. It is an associated
    /// token account owned by the escrow PDA, so only this program can move it.
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// The maker's own token-A account, which funds the deposit.
    #[account(
        mut,
        constraint = maker_ata_a.mint == mint_a.key() @ EscrowError::WrongMint,
        constraint = maker_ata_a.owner == maker.key() @ EscrowError::WrongOwner
    )]
    pub maker_ata_a: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Cancel<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub mint_a: Account<'info, Mint>,

    /// The offer being cancelled. `has_one` re-checks the stored maker/mint_a,
    /// `close = maker` refunds its rent, and the seeds prove it's the right PDA.
    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = mint_a,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = maker_ata_a.mint == mint_a.key() @ EscrowError::WrongMint,
        constraint = maker_ata_a.owner == maker.key() @ EscrowError::WrongOwner
    )]
    pub maker_ata_a: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,

    /// The maker receives the token-B payment and the reclaimed rent. Not a
    /// signer here — `has_one = maker` on the escrow pins it to the real maker,
    /// so the taker can't redirect the payout.
    #[account(mut)]
    pub maker: SystemAccount<'info>,

    pub mint_a: Account<'info, Mint>,
    pub mint_b: Account<'info, Mint>,

    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = mint_a,
        has_one = mint_b,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// Where the taker receives token A. `init_if_needed` creates it on demand;
    /// safe here because an ATA's address is deterministic and idempotent.
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_a,
        associated_token::authority = taker,
    )]
    pub taker_ata_a: Account<'info, TokenAccount>,

    /// The taker's token-B account, which funds the payment.
    #[account(
        mut,
        constraint = taker_ata_b.mint == mint_b.key() @ EscrowError::WrongMint,
        constraint = taker_ata_b.owner == taker.key() @ EscrowError::WrongOwner
    )]
    pub taker_ata_b: Account<'info, TokenAccount>,

    /// Where the maker receives token B. Created on demand if it doesn't exist.
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_b,
        associated_token::authority = maker,
    )]
    pub maker_ata_b: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
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
