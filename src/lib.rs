use std::ops::Sub;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{
    env, ext_contract, near_bindgen, require, AccountId, BorshStorageKey, Gas, PanicOnDefault,
};use near_sdk::collections::LookupMap;
use chrono::Utc;
use near_sdk::json_types::U128;

pub const FT_TRANSFER_GAS: Gas = Gas(10_000_000_000_000);
pub const WITHDRAW_CALLBACK_GAS: Gas = Gas(10_000_000_000_000);
pub const FAUCET_CALLBACK_GAS: Gas = Gas(10_000_000_000_000);

pub const POINT_ONE_TOKEN: u128 = 100_000_000_000_000_000_000_000; // 0.1 to 24 decimal
pub const ONE_TOKEN: u128 = 1_000_000_000_000_000_000_000_000;

#[ext_contract(ext_ft_contract)]
pub trait FungibleTokenCore {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    );
    fn ft_resolve_transfer(&mut self, sender_id: AccountId, receiver_id: AccountId, amount: U128);
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct StakeInfo {
    time_staked: i64,
    amount_staked: u128,
    reward: u128,
    apr: u128,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub total_stakers: u128,
    pub total_staked: u128,
    pub stake_info: LookupMap<AccountId, StakeInfo>,
    pub token_address: AccountId,
}

#[derive(BorshDeserialize, BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    StakeInfoKey,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// default metadata (for example purposes only).
    #[init]
    pub fn new(_token_address: AccountId) -> Self {
        Contract {
            token_address: _token_address,
            total_stakers: 0,
            total_staked: 0,
            stake_info: LookupMap::new(StorageKey::StakeInfoKey)
        }
    }

    pub fn stake_token(&mut self, _account_id: AccountId, _stake_amount: u128) {
        require!(_stake_amount > 0, "Stake: Invalid amount!");
        
        let info = self.stake_info.get(&_account_id);
        match info {
            Some(mut unwrap_info) => {
                unwrap_info.time_staked = Utc::now().timestamp();
                unwrap_info.amount_staked += _stake_amount;
                unwrap_info.reward += Self::pending_reward(&self, _account_id);
            },
            None => {
                let stake_info = StakeInfo {
                    time_staked: Utc::now().timestamp(),
                    amount_staked: _stake_amount,
                    reward: 0,
                    apr: 5_000_000_000_000_000_000_000_000,
                };
                self.stake_info.insert(&_account_id, &stake_info);
                self.total_stakers += 1;
            },
        }
        self.total_staked += _stake_amount;

        ext_ft_contract::ext(self.token_address.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .ft_transfer_call(
                env::current_account_id(),
                U128::from(_stake_amount),
                None,
                "spk_stake".to_string(),
            )
            .then(
                ext_ft_contract::ext(self.token_address.clone()).ft_resolve_transfer(
                    env::signer_account_id(),
                    env::current_account_id(),
                    U128::from(_stake_amount),
                ),
            );
    }

    pub fn unstake_token(&mut self, _account_id: AccountId, _amount: u128) {
        require!(self.stake_info.contains_key(&_account_id) == true, "Stake: You didn't stake any tokens!");
        let mut stake_info = self.stake_info.get(&_account_id).unwrap();
        require!(stake_info.amount_staked > 0, "Stake: You staked less token than amount");
        require!(_amount > 0, "Stake: Invalid amount");

        stake_info.amount_staked -= _amount;
        stake_info.time_staked = Utc::now().timestamp();
        stake_info.reward += Self::pending_reward(&self, _account_id);

        ext_ft_contract::ext(self.token_address.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .ft_transfer(
                env::signer_account_id(),
                U128::from(_amount),
                None,
            );

        self.total_staked -= _amount;
    }

    pub fn claim_reward(&self, _account_id: AccountId) {
        require!(self.stake_info.contains_key(&_account_id) == true, "Stake: You didn't stake any tokens!");
        let mut stake_info = self.stake_info.get(&_account_id).unwrap();

        let reward = Self::pending_reward(&self, _account_id);
        require!(reward > 0, "Stake: You have no reward yet!");

        ext_ft_contract::ext(self.token_address.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .ft_transfer(
                env::signer_account_id(),
                U128::from(reward),
                None,
            );

        stake_info.time_staked = Utc::now().timestamp();
        stake_info.reward = 0;
    }

    pub fn pending_reward(&self, _account_id: AccountId) -> u128 {
        require!(self.stake_info.contains_key(&_account_id) == true, "Stake: You didn't stake any tokens!");
        let stake_info = self.stake_info.get(&_account_id).unwrap();

        let time_last = Utc::now().timestamp().sub(stake_info.time_staked);
        let pending_reward = (stake_info.amount_staked * (time_last as u128) * stake_info.apr)/(31536000*ONE_TOKEN);
        return pending_reward + stake_info.reward;
    }

    pub fn get_staked_amount(&self, _advisor_id: AccountId) -> u128 {
        require!(self.stake_info.contains_key(&_advisor_id) == true, "Stake: Advisor not stake any tokens!");
        return self.stake_info.get(&_advisor_id).unwrap().amount_staked;
    }

    pub fn update_apr(&self, _advisor_id: AccountId, _learner_vote: u8) {
        require!(self.stake_info.contains_key(&_advisor_id) == true, "Stake: Advisor not stake any tokens!");
        let mut stake_info = self.stake_info.get(&_advisor_id).unwrap();
        stake_info.reward = self.pending_reward(_advisor_id);
        stake_info.time_staked = Utc::now().timestamp();
        match _learner_vote {
            1_u8 => {
                stake_info.apr -= POINT_ONE_TOKEN*2;
            },
            2_u8 => {
                stake_info.apr -= POINT_ONE_TOKEN;
            },
            3_u8 => {
                // stake_info.apr -= POINT_ONE_TOKEN;
            },
            4_u8 => {
                stake_info.apr += POINT_ONE_TOKEN;
            },
            5_u8 => {
                stake_info.apr += POINT_ONE_TOKEN*2;
            },
            _ => {

            }
        }
    }
}