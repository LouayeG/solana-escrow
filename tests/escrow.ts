import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Escrow } from "../target/types/escrow";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { assert } from "chai";

describe("escrow", () => {
  // Wallet + cluster come from Anchor.toml.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.escrow as Program<Escrow>;
  const connection = provider.connection;

  const maker = provider.wallet as anchor.Wallet;
  const taker = Keypair.generate();

  const DECIMALS = 6;
  const DEPOSIT = new BN(100_000_000); // 100 token A the maker locks
  const RECEIVE = new BN(50_000_000); //  50 token B the maker wants

  let mintA: PublicKey; // what the maker deposits
  let mintB: PublicKey; // what the maker wants back
  let makerAtaA: PublicKey;
  let takerAtaB: PublicKey;

  // The offer PDA for a given seed, and the vault ATA it owns.
  const escrowPda = (seed: number) =>
    PublicKey.findProgramAddressSync(
      [
        Buffer.from("escrow"),
        maker.publicKey.toBuffer(),
        new BN(seed).toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    )[0];
  const vaultFor = (escrow: PublicKey) =>
    getAssociatedTokenAddressSync(mintA, escrow, true); // allowOwnerOffCurve: PDA

  before(async () => {
    // Fund the taker with SOL for fees + rent.
    await connection.confirmTransaction(
      await connection.requestAirdrop(taker.publicKey, 5 * LAMPORTS_PER_SOL)
    );

    // Two brand-new tokens; the maker is the mint authority for both.
    mintA = await createMint(connection, maker.payer, maker.publicKey, null, DECIMALS);
    mintB = await createMint(connection, maker.payer, maker.publicKey, null, DECIMALS);

    // Maker holds token A; taker holds token B.
    makerAtaA = (
      await getOrCreateAssociatedTokenAccount(connection, maker.payer, mintA, maker.publicKey)
    ).address;
    takerAtaB = (
      await getOrCreateAssociatedTokenAccount(connection, maker.payer, mintB, taker.publicKey)
    ).address;

    await mintTo(connection, maker.payer, mintA, makerAtaA, maker.publicKey, 1_000_000_000);
    await mintTo(connection, maker.payer, mintB, takerAtaB, maker.publicKey, 1_000_000_000);
  });

  it("sets up mints and funded token accounts", async () => {
    assert.equal(Number((await getAccount(connection, makerAtaA)).amount), 1_000_000_000);
    assert.equal(Number((await getAccount(connection, takerAtaB)).amount), 1_000_000_000);
  });
});
