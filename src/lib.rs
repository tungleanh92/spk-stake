use std::ops::Sub;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, near_bindgen, require, AccountId, BorshStorageKey, Gas, PanicOnDefault,
    ONE_NEAR, ONE_YOCTO, PromiseOrValue,
};

pub const FT_TRANSFER_GAS: Gas = Gas(10_000_000_000_000);
pub const WITHDRAW_CALLBACK_GAS: Gas = Gas(10_000_000_000_000);
pub const FAUCET_CALLBACK_GAS: Gas = Gas(10_000_000_000_000);

pub const POINT_ONE_TOKEN: u128 = 100_000_000_000_000_000_000_000; // 0.1 to 24 decimal
pub const DEFAULT_APR: u128 = 5_000_000_000_000_000_000_000_000; // 5%

pub mod external;
pub use crate::external::*;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct StakeInfo {
    time_staked: i64,
    amount_staked: u128,
    reward: u128,
    apr: u128,
    votes: u8,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub token_address: AccountId,
    pub total_stakers: u128,
    pub total_staked: u128,
    pub stake_info: LookupMap<AccountId, StakeInfo>,
}

#[derive(BorshDeserialize, BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    StakeInfoKey,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(_token_address: AccountId) -> Self {
        Contract {
            token_address: _token_address,
            total_stakers: 0,
            total_staked: 0,
            stake_info: LookupMap::new(StorageKey::StakeInfoKey),
        }
    }

    // call ft_transfer_call on token contract to do stake_token fn called by token contract
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let _stake_amount = u128::from(amount);
        let _account_id = sender_id;
        require!(_stake_amount > 0, "Stake: Invalid amount!");

        let info = self.stake_info.get(&_account_id);
        match info {
            Some(mut unwrap_info) => {
                unwrap_info.time_staked = Self::now();
                unwrap_info.amount_staked += _stake_amount;
                unwrap_info.reward += Self::pending_reward(&self, _account_id.clone());

                self.stake_info.insert(&_account_id, &unwrap_info);
            }
            None => {
                let stake_info = StakeInfo {
                    time_staked: Self::now(),
                    amount_staked: _stake_amount,
                    reward: 0,
                    apr: DEFAULT_APR,
                    votes: 0,
                };
                self.stake_info.insert(&_account_id, &stake_info);
                self.total_stakers += 1;
            }
        }
        self.total_staked += _stake_amount;

        return PromiseOrValue::Value(near_sdk::json_types::U128(0));
    }

    #[payable]
    pub fn unstake_token(&mut self, _amount: U128) {
        assert_one_yocto();
        let _amount = u128::from(_amount);
        let _account_id = env::signer_account_id();
        require!(
            self.stake_info.contains_key(&_account_id) == true,
            "Stake: You didn't stake any tokens!"
        );
        let mut stake_info = self.stake_info.get(&_account_id).unwrap();
        require!(
            stake_info.amount_staked > 0,
            "Stake: You staked less token than amount"
        );
        require!(_amount > 0, "Stake: Invalid amount");

        ext_ft_contract::ext(self.token_address.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer(env::signer_account_id(), U128::from(_amount), None);

        stake_info.amount_staked -= _amount;
        stake_info.time_staked = Self::now();
        stake_info.reward += Self::pending_reward(&self, _account_id.clone());

        self.total_staked -= _amount;

        self.stake_info.insert(&_account_id, &stake_info);
    }

    #[payable]
    pub fn claim_reward(&mut self) {
        assert_one_yocto();
        let _account_id = env::signer_account_id();
        require!(
            self.stake_info.contains_key(&_account_id) == true,
            "Stake: You didn't stake any tokens!"
        );
        let mut stake_info = self.stake_info.get(&_account_id).unwrap();

        let reward = Self::pending_reward(&self, _account_id.clone());
        require!(reward > 0, "Stake: You have no reward yet!");

        ext_ft_contract::ext(self.token_address.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer(env::signer_account_id(), U128::from(reward), None);

        stake_info.time_staked = Self::now();
        stake_info.reward = 0;

        self.stake_info.insert(&_account_id, &stake_info);
    }

    pub fn pending_reward(&self, _account_id: AccountId) -> u128 {
        require!(
            self.stake_info.contains_key(&_account_id) == true,
            "Stake: You didn't stake any tokens!"
        );
        let stake_info = self.stake_info.get(&_account_id).unwrap();

        let time_last = Self::now().sub(stake_info.time_staked);
        let pending_reward = (stake_info.amount_staked * (time_last as u128) / (31536000 * 100))
            * stake_info.apr
            / ONE_NEAR;
        return pending_reward + stake_info.reward;
    }

    pub fn get_staked_amount(&self, _advisor_id: AccountId) -> u128 {
        require!(
            self.stake_info.contains_key(&_advisor_id) == true,
            "Stake: Advisor not stake any tokens!"
        );
        return self.stake_info.get(&_advisor_id).unwrap().amount_staked;
    }

    pub fn update_apr(&mut self, _advisor_id: AccountId, _learner_vote: u8) {
        require!(
            self.stake_info.contains_key(&_advisor_id) == true,
            "Stake: Advisor not stake any tokens!"
        );
        let mut stake_info = self.stake_info.get(&_advisor_id).unwrap();
        stake_info.reward = Self::pending_reward(&self, _advisor_id.clone());
        stake_info.time_staked = Self::now();
        match _learner_vote {
            1_u8 => {
                stake_info.apr -= POINT_ONE_TOKEN * 2;
                stake_info.votes -= 2;
            }
            2_u8 => {
                stake_info.apr -= POINT_ONE_TOKEN;
                stake_info.votes -= 1;
            }
            3_u8 => {
                // do nothing
            }
            4_u8 => {
                stake_info.apr += POINT_ONE_TOKEN;
                stake_info.votes += 1;
            }
            5_u8 => {
                stake_info.apr += POINT_ONE_TOKEN * 2;
                stake_info.votes += 2;
            }
            _ => {
                require!(1 != 1, "Stake: Invalid vote!");
            }
        }
        self.stake_info.insert(&_advisor_id, &stake_info);
    }

    #[private]
    pub fn now() -> i64 {
        return env::block_timestamp() as i64;
    }
}
