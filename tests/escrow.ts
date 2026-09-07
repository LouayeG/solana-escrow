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

  it("make: opens an offer and locks token A", async () => {
    const seed = 1;
    const escrow = escrowPda(seed);
    const vault = vaultFor(escrow);

    await program.methods
      .make(new BN(seed), DEPOSIT, RECEIVE)
      .accountsPartial({
        maker: maker.publicKey,
        mintA,
        mintB,
        escrow,
        vault,
        makerAtaA,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    // The deposit is now held in the vault.
    assert.equal(Number((await getAccount(connection, vault)).amount), DEPOSIT.toNumber());

    // The offer's terms are recorded on-chain.
    const state = await program.account.escrow.fetch(escrow);
    assert.equal(state.maker.toBase58(), maker.publicKey.toBase58());
    assert.equal(state.mintA.toBase58(), mintA.toBase58());
    assert.equal(state.mintB.toBase58(), mintB.toBase58());
    assert.equal(state.receive.toNumber(), RECEIVE.toNumber());
  });

  it("take: fills the offer atomically", async () => {
    const seed = 1; // the offer opened in the make test
    const escrow = escrowPda(seed);
    const vault = vaultFor(escrow);
    const takerAtaA = getAssociatedTokenAddressSync(mintA, taker.publicKey);
    const makerAtaB = getAssociatedTokenAddressSync(mintB, maker.publicKey);

    await program.methods
      .take()
      .accountsPartial({
        taker: taker.publicKey,
        maker: maker.publicKey,
        mintA,
        mintB,
        escrow,
        vault,
        takerAtaA,
        takerAtaB,
        makerAtaB,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([taker])
      .rpc();

    // Both legs settled: taker received token A, maker received token B.
    assert.equal(Number((await getAccount(connection, takerAtaA)).amount), DEPOSIT.toNumber());
    assert.equal(Number((await getAccount(connection, makerAtaB)).amount), RECEIVE.toNumber());

    // The offer is fully wound down.
    assert.isNull(await connection.getAccountInfo(escrow), "escrow should be closed");
    assert.isNull(await connection.getAccountInfo(vault), "vault should be closed");
  });

  it("cancel: maker reclaims the deposit", async () => {
    const seed = 2; // a fresh offer just for this test
    const escrow = escrowPda(seed);
    const vault = vaultFor(escrow);

    const balanceBefore = Number((await getAccount(connection, makerAtaA)).amount);

    await program.methods
      .make(new BN(seed), DEPOSIT, RECEIVE)
      .accountsPartial({
        maker: maker.publicKey,
        mintA,
        mintB,
        escrow,
        vault,
        makerAtaA,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .cancel()
      .accountsPartial({
        maker: maker.publicKey,
        mintA,
        escrow,
        vault,
        makerAtaA,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    // The deposit came all the way back — net zero for the maker.
    const balanceAfter = Number((await getAccount(connection, makerAtaA)).amount);
    assert.equal(balanceAfter, balanceBefore);
    assert.isNull(await connection.getAccountInfo(escrow), "escrow should be closed");
    assert.isNull(await connection.getAccountInfo(vault), "vault should be closed");
  });
});
