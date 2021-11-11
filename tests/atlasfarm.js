const anchor = require('@project-serum/anchor');
const moment = require('moment');
const assert = require("assert");
const { SystemProgram } = anchor.web3;

describe('atlasfarm', () => {
  const provider = anchor.Provider.env();
  // Configure the client to use the local cluster.
  anchor.setProvider(provider);
  const program = anchor.workspace.Atlasfarm;

  it("Creates a counter)", async () => {
    /* Call the create function via RPC */
    const baseAccount = anchor.web3.Keypair.generate();
    await program.rpc.create({
      accounts: {
        baseAccount: baseAccount.publicKey,
        user: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      },
      signers: [baseAccount],
    });

    /* Fetch the account and check the value of count */
    const account = await program.account.baseAccount.fetch(baseAccount.publicKey);
    console.log('Count 0: ', account.count.toString())
    assert.ok(account.count.toString() == 0);
    _baseAccount = baseAccount;

  });

  it('Creating a farmpool!', async () => {
    // Add your test here.
    let current = moment().unix();
    // let future10days = current + 10 * 24 * 3600;
    let reward_rate = 10;

    const pool_acct = anchor.web3.Keypair.generate();

    console.log('pool_acct', pool_acct.publicKey);
    console.log('authority',provider.wallet.publicKey );
    console.log(program.rpc);

    const tx = await program.rpc.initPoolAcct(
      current,
      reward_rate,
      {
      accounts: {
        poolAcct: pool_acct.publicKey,
        authority: provider.wallet.publicKey
      },
      signers: [pool_acct]
    });
    console.log('transaction', tx);

    _pool_acct = pool_acct;
    console.log('pool account', _pool_acct);
    // const tx = await program.rpc.initialize();
    // console.log("Your transaction signature", tx);
  });
});
