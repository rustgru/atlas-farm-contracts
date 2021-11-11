use anchor_lang::prelude::msg;

pub fn calculate_reward(stake_amount: u64, total_pool_deposit: u64, alloc_point: u16, total_alloc_point: u16, reward_rate: u64, time_now: u32, stake_prev: u32) -> u64 {
    msg!("--------------== calculate_reward");

    msg!("stake_amount = {}, total_pool_deposit = {}", stake_amount, total_pool_deposit);

    let alloc_point_ratio = alloc_point.checked_div(total_alloc_point).expect("eCal2");//u16
    msg!("alloc_point = {}, total_alloc_point = {}, alloc_point_ratio = {}", alloc_point, total_alloc_point, alloc_point_ratio);

    let staking_period = time_now.checked_sub(stake_prev).expect("eCal1");
    msg!("stake_prev = {}, deposit_now = {}, staking_period = {}", stake_prev, time_now, staking_period);

    //user reward = user deposit/total deposit in the pool * allocationPoint/Total allocationPoint from all Pools * # of Aries per sec*staking peroid in seconds
    let mut reward_debt = 0;
    if total_pool_deposit > 0 {
      reward_debt = stake_amount.checked_div(total_pool_deposit).expect("eCal3").checked_mul(u64::from(alloc_point_ratio)).expect("eCal4").checked_mul(reward_rate).expect("eCal5").checked_mul(u64::from(staking_period)).expect("eCal6");
    }
    msg!("reward_debt: {}", reward_debt);
    reward_debt
  }