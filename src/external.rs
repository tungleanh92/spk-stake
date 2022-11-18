use near_sdk::json_types::U128;
use near_sdk::{ext_contract, AccountId};

#[ext_contract(ext_ft_contract)]
pub trait FungibleTokenCore {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}
