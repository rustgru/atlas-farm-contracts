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

let stateSigner, stateBump;


let lpMint;
let lpPoolSigner, lpPoolBump;
let lpPoolVault, lpPoolVaultBump;

let rewardMint;
let rewardPoolVault, rewardPoolVaultBump;

let userSigner, userBump;

anchor.setProvider(provider);

let program = anchor.workspace.AtlasFarming
let connection = provider.connection

const user = anchor.web3.Keypair.generate();
const master = anchor.web3.Keypair.generate();

let userLPToken;
let masterRewardToken;
let amount = new BN(20);

const defaultAccounts = {
  tokenProgram: TOKEN_PROGRAM_ID,
  clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
  systemProgram: SystemProgram.programId,
}

describe('atlas-farm', () => {
  it('Is initialized!', async function () {
    rewardMint = await createMint(provider, provider.wallet.publicKey);
    lpMint = await createMint(provider, provider.wallet.publicKey);

    console.log(rewardMint.toString(), lpMint.toString())

    [stateSigner, stateBump] = await anchor.web3.PublicKey.findProgramAddress(
      [utf8.encode('state')],
      program.programId
    );
    
    [lpPoolSigner, lpPoolBump] = await anchor.web3.PublicKey.findProgramAddress(
      [lpMint.publicKey.toBuffer()],
      program.programId
    );

    [userSigner, userBump] = await anchor.web3.PublicKey.findProgramAddress(
      [lpPoolSigner.publicKey.toBuffer(), user.publicKey.toBuffer()],
      program.programId
    );

    [lpPoolVault, lpPoolVaultBump] = await anchor.web3.PublicKey.findProgramAddress(
      [utf8.encode('pool vault'), lpMint.publicKey.toBuffer(), lpPoolSigner.pubkey.toBuffer()],
      program.programId
    );
    [rewardPoolVault, rewardPoolVaultBump] = await anchor.web3.PublicKey.findProgramAddress(
      [utf8.encode('reward vault'), rewardMint.publicKey.toBuffer(), lpPoolSigner.pubkey.toBuffer()],
      program.programId
    );
    masterRewardToken = await rewardMint.createAccount(master)
    
    userRewardToken = await rewardMint.createAccount(user)
    userLPToken = await lpMint.createAccount(user)
    
    await rewardMint.mintTo(masterRewardToken, master, [provider.wallet], new BN(1000000000).toString())
    await lpMint.mintTo(userLPToken, master, [provider.wallet], new BN(100).toString())

  })

  it('Create State', async function () {
    await program.rpc.createState(stateBump, {
      accounts: {
        state: stateSigner,
        authority: master.publicKey,
        ...defaultAccounts
      },
      signers:[master]
    })
    const stateInfo = await program.account.stateAccount.fetch(stateSigner)
    console.log(stateInfo)
  })

  it('Create Pool', async function () {
    await program.rpc.createPool(poolBump,  lpPoolVaultBump, rewardPoolVaultBump, new BN('20'), {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        authority: master,
        tokenMint: lpMint.publicKey,
        tokenVault: lpPoolVault.publicKey,
        rewardMint: rewardMint.publicKey,
        rewardVault: rewardPoolVault.publicKey,
        ...defaultAccounts
      },
      signers: [master]
    })
  })
  
  it('ChangeTokenPerSecond', async function () {
    await program.rpc.changeTokenPerSecond(new BN(40), {
      accounts: {
        state: stateSigner,
        pool: poolSigner,
        authority: master.publicKey,
        ...defaultAccounts
      },
      signers: [master]
    })

    const poolInfo = await program.account.farmPoolAccount.fetch(poolSigner)
    assert.ok(poolInfo.tokenPerSecond.eq(new BN(40)), "Invalid change token per second")

  })

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
    await master.provider.send(tx, [], {})
  
    const rewardVaultAmount = await getTokenAmount(rewardPoolVault)
    assert.ok(rewardVaultAmount.eq(new BN(10000)))
  })

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
    await user.provider.send(tx, [], {})

    console.log(await program.account.farmPoolUserAccount.fetch(userSigner))
  })
  it('Stake', async function () {
    const tx = program.transaction.stake(amount, {
      accounts: {
        state: stateSigner,
        pool: lpPoolSigner,
        user: userSigner,

        poolVault: lpPoolVault,
        userVault: userLPToken,

        authority: user.publicKey,
        ...defaultAccounts
      }
    });

    await user.provider.send(tx, [], {})
    const userInfo = await program.account.farmPoolUserAccount.fetch(userSigner)
    assert.ok(userInfo.amount.eq(amount), "Invalid stake amount")
  })
  it('Harvest', async function () {
    
    await sleep(3000);
    const tx = program.transaction.harvest({
      accounts: {
        state: stateSigner,
        pool: lpPoolSigner,
        user: userSigner,

        rewardVault: poolRewardVault,
        userVault: userRewardToken,
        authority: user.publicKey,
        ...defaultAccounts
      }
    });
    await user.provider.send(tx, [], {})
    const rewardAmount = await getTokenAmount(userRewardToken)
    console.log(rewardAmount.toString());
  })

  it('Unstake', async function () {
    const tx = program.transaction.stake(amount,{
      accounts: {
        state: stateSigner,
        pool: lpPoolSigner,
        user: userSigner,

        poolVault: lpPoolVault,
        userVault: userLPToken,

        authority: user.publicKey,
        ...defaultAccounts
      }
    });
    await user.provider.send(tx, [], {})
    const userInfo = await program.account.farmPoolUserAccount.fetch(userSigner)
    assert.ok(userInfo.amount.eq(new BN(0)), "Invalid unstake amount")
  })
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