use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::convert::TryFrom;
use std::convert::TryInto;
use std::mem::size_of;

declare_id!("FBHjXGXUa65hSCzyfMhkcLzu2U3HByNqcWMuDUUHURLa");

const FULL_100: u64 = 100_000_000_000;
const ACC_PRECISION: u128 = 100_000_000_000;

#[program]
pub mod atlas_farming {
    use super::*;

    pub fn create_state(
        _ctx: Context<CreateGlobalState>,
        bump: u8,
        token_per_second: u64,
    ) -> ProgramResult {
        let state = &mut _ctx.accounts.state.load_init()?;
        state.authority = _ctx.accounts.authority.key();
        state.bump = bump;
        state.start_time = _ctx.accounts.clock.unix_timestamp;
        Ok(())
    }

    pub fn create_pool(
        _ctx: Context<CreateFarmPool>,
        bump: u8,
        token_per_second: u64,
    ) -> ProgramResult {
        let mut state = _ctx.accounts.state.load_mut()?;
        for pool_acc in _ctx.remaining_accounts.iter() {
            let loader = Loader::<FarmPoolAccount>::try_from(&_ctx.program_id, &pool_acc)?;
            loader.load_mut()?.update(&state, &_ctx.accounts.clock)?;
        }

        let pool = &mut _ctx.accounts.pool.load_init()?;
        pool.bump = bump;
        pool.mint = _ctx.accounts.mint.key();
        pool.vault = _ctx.accounts.vault.key();
        pool.authority = _ctx.accounts.authority.key();
        
        pool.token_per_second = token_per_second;
        pool.reward_mint = _ctx.accounts.reward_mint.key();
        pool.reward_vault = _ctx.accounts.reward_vault.key();

        state.total_farm = state.total_farm.checked_add(1).unwrap();
        
        Ok(())
    }

    pub fn close_pool(_ctx: Context<CloseFarmPool>) -> ProgramResult {
        let mut state = _ctx.accounts.state.load_mut()?;
        for pool_acc in _ctx.remaining_accounts.iter() {
            let loader = Loader::<FarmPoolAccount>::try_from(&_ctx.program_id, &pool_acc)?;
            loader.load_mut()?.update(&state, &_ctx.accounts.clock)?;
        }
        let pool = _ctx.accounts.pool.load()?;
        require!(pool.amount == 0, ErrorCode::WorkingPool);
        state.total_farm = state.total_farm.checked_sub(1).unwrap();
        Ok(())
    }

    pub fn fund_reward_token(_ctx: Context<FundToFarm>, amount: u64) -> ProgramResult {
        let cpi_accounts = Transfer {
            from: _ctx.accounts.user_vault.to_account_info(),
            to: _ctx.accounts.reward_vault.to_account_info(),
            authority: _ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = _ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;
        Ok(())
    }

    pub fn change_tokens_per_second(
        _ctx: Context<ChangeEmissionRate>,
        token_per_second: u64,
    ) -> ProgramResult {
        let mut pool = _ctx.accounts.pool.load_mut()?;
        for pool_acc in _ctx.remaining_accounts.iter() {
            let loader = Loader::<FarmPoolAccount>::try_from(&_ctx.program_id, &pool_acc)?;
            loader.load_mut()?.update(&state, &_ctx.accounts.clock)?;
        }
        pool.token_per_second = token_per_second;
        Ok(())
    }


    pub fn create_user(_ctx: Context<CreateFarmUser>, bump: u8) -> ProgramResult {
        let user = &mut _ctx.accounts.user.load_init()?;
        user.authority = _ctx.accounts.authority.key();
        user.bump = bump;
        user.pool = _ctx.accounts.pool.key();

        let mut pool = _ctx.accounts.pool.load_mut()?;
        pool.total_user += 1;
        
        Ok(())
    }

    pub fn stake(_ctx: Context<Stake>, amount: u64, lock_duration: i64) -> ProgramResult {
        let state = _ctx.accounts.state.load()?;
        let extra_account = &mut _ctx.accounts.extra_reward_account;
        let mut user = _ctx.accounts.user.load_mut()?;
        let mut pool = _ctx.accounts.pool.load_mut()?;

        extra_account.validate_lock_duration(&lock_duration)?;
        require!(
            lock_duration >= user.lock_duration,
            ErrorCode::InvalidLockDuration
        );

        pool.update(&state, &_ctx.accounts.clock)?;
        let user_lock_duration = user.lock_duration;
        user.calculate_reward_amount(&pool, &extra_account.get_extra_reward_percentage(&user_lock_duration))?;

        user.amount = user.amount.checked_add(amount).unwrap();
        pool.amount = pool.amount.checked_add(amount).unwrap();

        user.lock_duration = lock_duration;
        user.calculate_reward_debt(&pool)?;
        user.last_stake_time = _ctx.accounts.clock.unix_timestamp;

        let cpi_accounts = Transfer {
            from: _ctx.accounts.user_vault.to_account_info(),
            to: _ctx.accounts.pool_vault.to_account_info(),
            authority: _ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = _ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;
        
        Ok(())
    }

    pub fn unstake(_ctx: Context<Stake>, amount: u64) -> ProgramResult {
        let extra_account = &mut _ctx.accounts.extra_reward_account;
        let state = _ctx.accounts.state.load()?;
        let mut user = _ctx.accounts.user.load_mut()?;
        let mut pool = _ctx.accounts.pool.load_mut()?;

        require!(user.amount >= amount, ErrorCode::UnstakeOverAmount);
        require!(
            user.last_stake_time
                .checked_add(user.lock_duration)
                .unwrap()
                <= _ctx.accounts.clock.unix_timestamp,
            ErrorCode::UnderLocked
        );

        pool.update(&state, &_ctx.accounts.clock)?;
        let user_lock_duration = user.lock_duration;
        user.calculate_reward_amount(&pool, &extra_account.get_extra_reward_percentage(&user_lock_duration))?;

        user.last_stake_time = _ctx.accounts.clock.unix_timestamp;
        user.amount = user.amount.checked_sub(amount).unwrap();
        pool.amount = pool.amount.checked_sub(amount).unwrap();

        if user.amount == 0
        {
            user.lock_duration = 0;
        }

        user.calculate_reward_debt(&pool)?;
        drop(pool);

        let new_pool = _ctx.accounts.pool.load()?;
        let cpi_accounts = Transfer {
            from: _ctx.accounts.pool_vault.to_account_info(),
            to: _ctx.accounts.user_vault.to_account_info(),
            authority: _ctx.accounts.pool.to_account_info(),
        };

        let seeds = &[new_pool.mint.as_ref(), &[new_pool.bump]];
        let signer = &[&seeds[..]];
        let cpi_program = _ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;
        
        Ok(())
    }

    pub fn harvest(_ctx: Context<Harvest>) -> ProgramResult {
        let extra_account = &mut _ctx.accounts.extra_reward_account;
        let state = _ctx.accounts.state.load()?;
        let mut pool = _ctx.accounts.pool.load_mut()?;
        let mut user = _ctx.accounts.user.load_mut()?;

        pool.update(&state, &_ctx.accounts.clock)?;
        let user_lock_duration = user.lock_duration;
        user.calculate_reward_amount(&pool, &extra_account.get_extra_reward_percentage(&user_lock_duration))?;

        let total_reward = user.reward_amount.checked_add(user.extra_reward).unwrap().try_into().unwrap();

        let cpi_accounts = Transfer {
            from: _ctx.accounts.reward_vault.to_account_info(),
            to: _ctx.accounts.user_vault.to_account_info(),
            authority: _ctx.accounts.state.to_account_info(),
        };

        let seeds = &[b"state".as_ref(), &[state.bump]];
        let signer = &[&seeds[..]];
        let cpi_program = _ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, total_reward)?;

        user.reward_amount = 0;
        user.extra_reward = 0;
        user.calculate_reward_debt(&pool)?;
        
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct CreateGlobalState<'info> {
    #[account(
        init,
        seeds = [b"state".as_ref()],
        bump = bump,
        payer = authority,
        space = 8 + size_of::<GlobalStateAccount>()
    )]
    pub state: Loader<'info, GlobalStateAccount>,
    pub authority: Signer<'info>,
    pub system_program: UncheckedAccount<'info>,
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct CreateFarmPool<'info> {
    #[account(
        init,
        seeds = [mint.key().as_ref()],
        bump = bump,
        payer = authority,
        space = 8 + size_of::<FarmPoolAccount>()
    )]
    pub pool: Loader<'info, FarmPoolAccount>,
    #[account(mut, seeds = [b"state".as_ref()], bump = state.load()?.bump, has_one = authority)]
    pub state: Loader<'info, GlobalStateAccount>,

    pub mint: Box<Account<'info, Mint>>,
    #[account(constraint = vault.owner == pool.key())]
    pub vault: Account<'info, TokenAccount>,
    
    pub authority: Signer<'info>,
    
    #[account(constraint = reward_vault.owner == pool.key())]
    pub reward_vault: Account<'info, TokenAccount>,
    pub reward_mint: Box<Account<'info, Mint>>,
    
    pub system_program: UncheckedAccount<'info>,
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct CloseFarmPool<'info> {
    #[account(mut, seeds = [b"state".as_ref()], bump = state.load()?.bump, has_one = authority)]
    pub state: Loader<'info, GlobalStateAccount>,
    #[account(mut, seeds = [pool.load()?.mint.key().as_ref()], bump = pool.load()?.bump, has_one = authority, close = authority)]
    pub pool: Loader<'info, FarmPoolAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: UncheckedAccount<'info>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct FundToFarm<'info> {
    #[account(seeds = [b"state".as_ref()], bump = state.load()?.bump)]
    pub state: Loader<'info, GlobalStateAccount>,
    pub authority: Signer<'info>,

    #[account(mut, seeds = [pool.load()?.mint.key().as_ref()], bump = pool.load()?.bump, has_one = authority, close = authority)]
    pub pool: Loader<'info, FarmPoolAccount>,

    #[account(mut, constraint = reward_vault.owner == pool.key())]
    pub reward_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut, constraint = user_vault.owner == authority.key())]
    pub user_vault: Box<Account<'info, TokenAccount>>,
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ChangeEmissionRate<'info> {
    #[account(mut, seeds = [b"state".as_ref()], bump = state.load()?.bump, has_one = authority)]
    pub state: Loader<'info, GlobalStateAccount>,
    pub authority: Signer<'info>,

    #[account(mut, seeds = [pool.load()?.mint.key().as_ref()], bump = pool.load()?.bump, has_one = authority, close = authority)]
    pub pool: Loader<'info, FarmPoolAccount>,

    #[account(constraint = reward_vault.owner == pool.key())]
    pub reward_vault: Account<'info, TokenAccount>,
    pub reward_mint: Box<Account<'info, Mint>>,

    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct CreateFarmUser<'info> {
    #[account(
        init,
        seeds = [pool.key().as_ref(), authority.key().as_ref()],
        bump = bump,
        payer = authority,
        space = 8 + size_of::<FarmPoolUserAccount>()
    )]
    pub user: Loader<'info, FarmPoolUserAccount>,
    #[account(seeds = [b"state".as_ref()], bump = state.load()?.bump)]
    pub state: Loader<'info, GlobalStateAccount>,
    #[account(mut, seeds = [pool.load()?.mint.key().as_ref()], bump = pool.load()?.bump)]
    pub pool: Loader<'info, FarmPoolAccount>,
    pub authority: Signer<'info>,
    pub system_program: UncheckedAccount<'info>,
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut, seeds = [pool.key().as_ref(), authority.key().as_ref()], bump = user.load()?.bump, has_one = pool, has_one = authority)]
    pub user: Loader<'info, FarmPoolUserAccount>,
    #[account(mut, seeds = [b"state".as_ref()], bump = state.load()?.bump)]
    pub state: Loader<'info, GlobalStateAccount>,
    #[account(seeds = [b"extra".as_ref()], bump = extra_reward_account.bump)]
    pub extra_reward_account: Box<Account<'info, ExtraRewardsAccount>>,
    #[account(mut, seeds = [pool.load()?.mint.key().as_ref()], bump = pool.load()?.bump)]
    pub pool: Loader<'info, FarmPoolAccount>,
    pub authority: Signer<'info>,
    #[account(constraint = mint.key() == pool.load()?.mint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, constraint = pool_vault.owner == pool.key())]
    pub pool_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut, constraint = user_vault.owner == authority.key())]
    pub user_vault: Box<Account<'info, TokenAccount>>,
    pub system_program: UncheckedAccount<'info>,
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct Harvest<'info> {
    #[account(mut, seeds = [pool.key().as_ref(), authority.key().as_ref()], bump = user.load()?.bump, has_one = pool, has_one = authority)]
    pub user: Loader<'info, FarmPoolUserAccount>,
    #[account(mut, seeds = [b"state".as_ref()], bump = state.load()?.bump)]
    pub state: Loader<'info, GlobalStateAccount>,
    #[account(seeds = [b"extra".as_ref()], bump = extra_reward_account.bump)]
    pub extra_reward_account: Box<Account<'info, ExtraRewardsAccount>>,
    #[account(mut, seeds = [pool.load()?.mint.key().as_ref()], bump = pool.load()?.bump)]
    pub pool: Loader<'info, FarmPoolAccount>,
    pub authority: Signer<'info>,
    #[account(constraint = mint.key() == pool.load()?.mint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, constraint = reward_vault.owner == state.key())]
    pub reward_vault: Box<Account<'info, TokenAccount>>,
    #[account(mut, constraint = user_vault.owner == authority.key())]
    pub user_vault: Box<Account<'info, TokenAccount>>,
    pub system_program: UncheckedAccount<'info>,
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[account(zero_copy)]
pub struct GlobalStateAccount {
    pub authority: Pubkey,
    pub bump: u8,
    pub total_farm: u64,
    pub start_time: i64,
}

#[account(zero_copy)]
pub struct FarmPoolAccount {
    pub bump: u8,
    pub authority: Pubkey,
    pub amount: u64,

    pub token_mint: Pubkey,
    pub token_vault: Pubkey,

    pub reward_mint: Pubkey,
    pub reward_vault: Pubkey,
    pub token_per_second: u64,

    pub last_reward_time: i64,
    pub acc_reward_per_share: u128,
    pub total_user: u64,
}

impl FarmPoolAccount {
    fn update<'info>(&mut self, state: &GlobalStateAccount, clock: &Sysvar<'info, Clock>) -> Result<()> {
        let seconds = u128::try_from(
            clock
                .unix_timestamp
                .checked_sub(self.last_reward_time)
                .unwrap(),
        )
        .unwrap();
        let mut reward_per_share: u128 = 0;
        if self.amount > 0 && seconds > 0 && self.point > 0 {
            reward_per_share = u128::from(state.token_per_second)
                .checked_mul(seconds)
                .unwrap()
                .checked_mul(u128::from(self.point))
                .unwrap()
                .checked_mul(ACC_PRECISION)
                .unwrap()
                .checked_div(u128::from(state.total_farm))
                .unwrap()
                .checked_div(u128::from(self.amount))
                .unwrap();
        }
        self.acc_reward_per_share = self
            .acc_reward_per_share
            .checked_add(reward_per_share)
            .unwrap();
        self.last_reward_time = clock.unix_timestamp;

        Ok(())
    }
}

#[account(zero_copy)]
pub struct FarmPoolUserAccount {
    pub bump: u8,
    pub pool: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
    pub reward_amount: u128,
    pub extra_reward: u128, // extra from lock duration; ex lock 12M => +10%
    pub reward_debt: u128,
    pub last_stake_time: i64,
    pub reserved_1: u128,
    pub reserved_2: u128,
    pub reserved_3: u128,
}

impl FarmPoolUserAccount {
    fn calculate_reward_amount<'info>(
        &mut self,
        pool: &FarmPoolAccount,
        extra_percentage: &u64,
    ) -> Result<()> {
        let pending_amount: u128 = u128::from(self.amount)
            .checked_mul(pool.acc_reward_per_share)
            .unwrap()
            .checked_div(ACC_PRECISION)
            .unwrap()
            .checked_sub(u128::from(self.reward_debt))
            .unwrap();
        self.reward_amount = self.reward_amount.checked_add(pending_amount).unwrap();
        let extra_amount: u128 = u128::from(pending_amount)
            .checked_mul(u128::from(*extra_percentage))
            .unwrap()
            .checked_div(u128::from(FULL_100))
            .unwrap();
        self.extra_reward = self.extra_reward.checked_add(extra_amount).unwrap();
        Ok(())
    }
    fn calculate_reward_debt<'info>(&mut self, pool: &FarmPoolAccount) -> Result<()> {

        self.reward_debt = u128::from(self.amount)
            .checked_mul(pool.acc_reward_per_share)
            .unwrap()
            .checked_div(ACC_PRECISION)
            .unwrap();
        Ok(())
    }
}

#[error]
pub enum ErrorCode {
    #[msg("Over staked amount")]
    UnstakeOverAmount,
    #[msg("Under locked")]
    UnderLocked,
    #[msg("Pool is working")]
    WorkingPool,
    #[msg("Invalid Lock Duration")]
    InvalidLockDuration,
    #[msg("Invalid SEQ")]
    InvalidSEQ,
}