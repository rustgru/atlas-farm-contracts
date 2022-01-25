const anchor = require('@project-serum/anchor');
const serumCmn = require("@project-serum/common");

const { TOKEN_PROGRAM_ID, Token, ASSOCIATED_TOKEN_PROGRAM_ID } = require("@solana/spl-token");
const _ = require('lodash')
const { BN, web3, Program, ProgramError, Provider } = anchor
const { PublicKey, SystemProgram, Keypair, Transaction } = web3
const assert = require("assert");
const utf8 = anchor.utils.bytes.utf8;
const provider = anchor.Provider.local()

const farmIdl = require('../target/idl/atlas_farming.json');
const { expect } = require('chai');
const { Connection } = require('@solana/web3.js');

let stateSigner = Keypair.generate().publicKey, stateBump = 255;

let lpMint = new Token();
let poolSigner = Keypair.generate().publicKey, poolBump = 255;
let poolVault = Keypair.generate().publicKey, poolVaultBump = 255;

let rewardMint = new Token();
let rewardPoolVault = Keypair.generate().publicKey, rewardPoolVaultBump= 255;

let userSigner = Keypair.generate().publicKey, userBump= 255;

anchor.setProvider(provider);

let program = anchor.workspace.AtlasFarming;
let connection = provider.connection;

const user = anchor.web3.Keypair.generate();
const user_provider = new anchor.Provider(connection, new anchor.Wallet(user));

const master = anchor.web3.Keypair.generate();
const master_provider = new anchor.Provider(connection, new anchor.Wallet(master));

let userLPToken;
let masterRewardToken;
let amount = new BN(20);

const defaultAccounts = {
  tokenProgram: TOKEN_PROGRAM_ID,
  clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
  systemProgram: SystemProgram.programId,
  rent: anchor.web3.SYSVAR_RENT_PUBKEY,
}

describe('atlas-farm', () => {
  it('Is initialized!', async function () {
    rewardMint = await createMint(provider, provider.wallet.publicKey);
    lpMint = await createMint(provider, provider.wallet.publicKey);

    [stateSigner, stateBump] = await PublicKey.findProgramAddress(
      [utf8.encode('state')],
      program.programId
    );
    
    [poolSigner, poolBump] = await PublicKey.findProgramAddress(
      [lpMint.publicKey.toBuffer()],
      program.programId
    );

    [userSigner, userBump] = await PublicKey.findProgramAddress(
      [poolSigner.toBuffer(), user.publicKey.toBuffer()],
      program.programId
    );

    [poolVault, poolVaultBump] = await PublicKey.findProgramAddress(
      [utf8.encode('pool-vault'), lpMint.publicKey.toBuffer(), poolSigner.toBuffer()],
      program.programId
    );

    [rewardPoolVault, rewardPoolVaultBump] = await PublicKey.findProgramAddress(
      [utf8.encode('reward-vault'), rewardMint.publicKey.toBuffer(), poolSigner.toBuffer()],
      program.programId
    );
    await connection.confirmTransaction(
      await connection.requestAirdrop(
        master.publicKey, web3.LAMPORTS_PER_SOL
      )
    )
    await connection.confirmTransaction(
      await connection.requestAirdrop(
        user.publicKey, web3.LAMPORTS_PER_SOL
      )
    )
    masterRewardToken = await rewardMint.createAccount(master.publicKey);
    userRewardToken = await rewardMint.createAccount(user.publicKey);
    userLPToken = await lpMint.createAccount(user.publicKey);
    
    await rewardMint.mintTo(masterRewardToken, provider.wallet, [], new BN(1000000000).toString());
    await lpMint.mintTo(userLPToken, provider.wallet, [], new BN(100).toString());
  });

  it('Create State', async function () {
    const tx = program.transaction.createState(stateBump, {
      accounts: {
        state: stateSigner,
        authority: master.publicKey,
        ...defaultAccounts
      }
    });
    await master_provider.send(tx, [], {})

    await program.account.globalStateAccount.fetch(stateSigner);
  })

  it('Create Pool', async function () {
    const tx = program.transaction.createPool(poolBump,  poolVaultBump, rewardPoolVaultBump, new BN('20'), {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        authority: master.publicKey,

        tokenMint: lpMint.publicKey,
        tokenVault: poolVault,

        rewardMint: rewardMint.publicKey,
        rewardVault: rewardPoolVault,
        ...defaultAccounts
      },
    })
    await master_provider.send(tx, [], {})

    await program.account.farmPoolAccount.fetch(poolSigner)
  })

  it('ChangeTokenPerSecond', async function () {

    const tx = program.transaction.changeTokenPerSecond(new BN(40), {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        authority: master.publicKey,
        ...defaultAccounts
      }
    })
    await master_provider.send(tx, [], {})

    poolInfo = await program.account.farmPoolAccount.fetch(poolSigner)
    assert.ok(poolInfo.tokenPerSecond.eq(new BN(40)), "Invalid change token per second")

  });

  it('Fund to farm', async function () {
    // await rewardMint.mintTo(stateRewardVault, creatorKey, [provider.wallet], getNumber(10000).toString())
    const tx = program.transaction.fundRewardToken(new BN(10000), {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        rewardVault: rewardPoolVault,
        userVault: masterRewardToken,
        authority: master.publicKey,
        ...defaultAccounts
      }
    })
    await master_provider.send(tx, [], {})

    const rewardVaultAmount = await getTokenAmount(rewardPoolVault)
    assert.ok(rewardVaultAmount.eq(new BN(10000)))
  });

  it('Create User', async function () {
    const tx = program.transaction.createUser(userBump, {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        user: userSigner,
        authority: user.publicKey,
        ...defaultAccounts
      }
    })
    await user_provider.send(tx, [], {})

    await program.account.farmPoolUserAccount.fetch(userSigner);
  })
  it('Stake', async function () {
    const tx = program.transaction.stake(amount, {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        user: userSigner,

        authority: user.publicKey,

        poolVault: poolVault,
        userVault: userLPToken,

        ...defaultAccounts
      }
    });

    await user_provider.send(tx, [], {})
    const userInfo = await program.account.farmPoolUserAccount.fetch(userSigner)
    assert.ok(userInfo.amount.eq(amount), "Invalid stake amount")
  })
  it('Harvest', async function () {
    
    console.log("Delaying 3 secs to harvest reward");
    await sleep(3000);
    const tx = program.transaction.harvest({
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        user: userSigner,

        authority: user.publicKey,

        rewardVault: rewardPoolVault,
        userVault: userRewardToken,
        ...defaultAccounts
      }
    });
    await user_provider.send(tx, [], {})
    const rewardAmount = await getTokenAmount(userRewardToken)
    console.log("Havested reward", rewardAmount.toString());
  })

  it('Unstake', async function () {
    const tx = program.transaction.unstake(amount, {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        user: userSigner,

        poolVault: poolVault,
        userVault: userLPToken,

        authority: user.publicKey,
        ...defaultAccounts
      }
    });
    await user_provider.send(tx, [], {})
    const userInfo = await program.account.farmPoolUserAccount.fetch(userSigner)
    assert.ok(userInfo.amount.eq(new BN(0)), "Invalid unstake amount")
  });

  it('Close Pool', async function () {
    const tx = program.transaction.closePool({
      accounts: {
        state: stateSigner,
        pool: poolSigner,

        authority: master.publicKey,
        ...defaultAccounts
      }
    });
    await master_provider.send(tx, [], {})
    const pools = await program.account.farmPoolAccount.all()
    assert.ok(pools.length == 0, "Pool isn't closed");
  });

})


/******************************************************** */
async function createMint (provider, authority, decimals = 9) {
  if (authority === undefined) {
    authority = provider.wallet.publicKey;
  }
  const mint = await Token.createMint(
    provider.connection,
    provider.wallet.payer,
    authority,
    null,
    decimals,
    TOKEN_PROGRAM_ID
  );
  return mint;
}

async function getTokenAmount (account) {
  const { amount } = await serumCmn.getTokenAccount(provider, account)
  return amount
}

function sleep (ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}