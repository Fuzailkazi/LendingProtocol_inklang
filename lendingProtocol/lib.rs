#![cfg_attr(not(feature = "std"), no_std)]

use ink::prelude::vec::Vec;
use ink::storage::Mapping;

/// Custom error types for the lending protocol
#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum Error {
    /// Occurs when an unauthorized user tries to perform an admin action
    NotAuthorized,
    /// Occurs when a user has insufficient balance for a transaction
    InsufficientBalance,
    /// Occurs when there's not enough liquidity in the protocol
    InsufficientLiquidity,
    /// Occurs when a user lacks sufficient collateral for a borrow
    InsufficientCollateral,
    /// Occurs when trying to interact with a paused contract
    ContractPaused,
}

/// Lending protocol smart contract
#[ink::contract]
mod lending_protocol {
    use super::*;

    /// Main contract structure storing protocol state
    #[ink(storage)]
    pub struct LendingProtocol {
        /// Account ID of the interest rate model contract
        interest_rate_model: AccountId,
        /// Account ID of the underlying asset contract
        underlying_asset: AccountId,
        /// Total amount of assets supplied to the protocol
        total_supply: Balance,
        /// Total amount of assets borrowed from the protocol
        total_borrow: Balance,
        /// Flag to pause/unpause the entire protocol
        paused: bool,
        /// Mapping of user balances (deposited assets)
        balances: Mapping<AccountId, Balance>,
        /// Mapping of user debt amounts
        debts: Mapping<AccountId, Balance>,
        /// Mapping of user collateral amounts
        collaterals: Mapping<AccountId, Balance>,
        /// Address of the protocol admin
        admin: AccountId,
    }

    /// Event emitted when the contract is initialized
    #[ink(event)]
    pub struct Initialized {
        #[ink(topic)]
        interest_rate_model: AccountId,
        #[ink(topic)]
        underlying_asset: AccountId,
    }

    /// Event emitted when assets are deposited
    #[ink(event)]
    pub struct Deposit {
        #[ink(topic)]
        from: AccountId,
        amount: Balance,
    }

    /// Event emitted when assets are withdrawn
    #[ink(event)]
    pub struct Withdraw {
        #[ink(topic)]
        to: AccountId,
        amount: Balance,
    }

    /// Event emitted when assets are borrowed
    #[ink(event)]
    pub struct Borrow {
        #[ink(topic)]
        borrower: AccountId,
        amount: Balance,
    }

    /// Event emitted when assets are repaid
    #[ink(event)]
    pub struct Repay {
        #[ink(topic)]
        borrower: AccountId,
        amount: Balance,
    }

    /// Event emitted during liquidation
    #[ink(event)]
    pub struct Liquidate {
        #[ink(topic)]
        liquidator: AccountId,
        #[ink(topic)]
        borrower: AccountId,
        amount: Balance,
    }

    /// Event emitted when interest is accrued
    #[ink(event)]
    pub struct InterestAccrued {
        amount: Balance,
    }

    /// Event emitted when interest rate model is updated
    #[ink(event)]
    pub struct InterestRateModelUpdated {
        #[ink(topic)]
        new_model: AccountId,
    }

    /// Event emitted when collateral is added
    #[ink(event)]
    pub struct CollateralAdded {
        #[ink(topic)]
        user: AccountId,
        amount: Balance,
    }

    /// Event emitted when collateral is removed
    #[ink(event)]
    pub struct CollateralRemoved {
        #[ink(topic)]
        user: AccountId,
        amount: Balance,
    }

    /// Event emitted when contract is paused
    #[ink(event)]
    pub struct ContractPaused;

    /// Event emitted when contract is unpaused
    #[ink(event)]
    pub struct ContractUnpaused;

    impl LendingProtocol {
        /// Constructor to create a new lending protocol instance
        #[ink(constructor)]
        pub fn new(interest_rate_model: AccountId, underlying_asset: AccountId) -> Self {
            let caller = Self::env().caller();
            
            // Emit initialization event
            Self::env().emit_event(Initialized {
                interest_rate_model,
                underlying_asset,
            });

            // Create and return the contract instance
            Self {
                interest_rate_model,
                underlying_asset,
                total_supply: 0,
                total_borrow: 0,
                paused: false,
                balances: Mapping::default(),
                debts: Mapping::default(),
                collaterals: Mapping::default(),
                admin: caller,
            }
        }

        /// Initialize or update the protocol's interest rate model and underlying asset
        #[ink(message)]
        pub fn initialize(&mut self, interest_rate_model: AccountId, underlying_asset: AccountId) -> Result<(), Error> {
            // Only admin can initialize
            self.only_admin()?;
            
            // Update interest rate model and underlying asset
            self.interest_rate_model = interest_rate_model;
            self.underlying_asset = underlying_asset;
            
            Ok(())
        }

        /// Deposit assets into the protocol
        #[ink(message)]
        pub fn deposit(&mut self, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let balance = self.balances.get(&caller).unwrap_or(0);
            
            // Update user balance and total supply
            self.balances.insert(&caller, &(balance + amount));
            self.total_supply += amount;
            
            // Emit deposit event
            self.env().emit_event(Deposit { from: caller, amount });
            
            Ok(())
        }

        /// Withdraw assets from the protocol
        #[ink(message)]
        pub fn withdraw(&mut self, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let balance = self.balances.get(&caller).unwrap_or(0);
            
            // Check for sufficient balance
            if balance < amount {
                return Err(Error::InsufficientBalance);
            }
            
            // Update user balance and total supply
            self.balances.insert(&caller, &(balance - amount));
            self.total_supply -= amount;
            
            // Emit withdraw event
            self.env().emit_event(Withdraw { to: caller, amount });
            
            Ok(())
        }

        /// Borrow assets from the protocol
        #[ink(message)]
        pub fn borrow(&mut self, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let collateral = self.collaterals.get(&caller).unwrap_or(0);
            let debt = self.debts.get(&caller).unwrap_or(0);
            
            // Calculate maximum borrowable amount based on collateral
            let max_borrow = self.calculate_max_borrow(collateral);
            
            // Check for sufficient collateral
            if max_borrow < debt + amount {
                return Err(Error::InsufficientCollateral);
            }
            
            // Update user debt and total borrow
            self.debts.insert(&caller, &(debt + amount));
            self.total_borrow += amount;
            
            // Emit borrow event
            self.env().emit_event(Borrow { borrower: caller, amount });
            
            Ok(())
        }

        /// Repay borrowed assets
        #[ink(message)]
        pub fn repay(&mut self, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let debt = self.debts.get(&caller).unwrap_or(0);
            
            // Check for sufficient debt to repay
            if debt < amount {
                return Err(Error::InsufficientBalance);
            }
            
            // Update user debt and total borrow
            self.debts.insert(&caller, &(debt - amount));
            self.total_borrow -= amount;
            
            // Emit repay event
            self.env().emit_event(Repay { borrower: caller, amount });
            
            Ok(())
        }

        /// Liquidate a borrower's position
        #[ink(message)]
        pub fn liquidate(&mut self, borrower: AccountId, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let debt = self.debts.get(&borrower).unwrap_or(0);
            
            // Check for sufficient debt to liquidate
            if debt < amount {
                return Err(Error::InsufficientBalance);
            }
            
            let collateral = self.collaterals.get(&borrower).unwrap_or(0);
            
            // Check for sufficient collateral
            if collateral < amount {
                return Err(Error::InsufficientCollateral);
            }
            
            // Update debt and collateral
            self.debts.insert(&borrower, &(debt - amount));
            self.collaterals.insert(&borrower, &(collateral - amount));
            
            // Emit liquidation event
            self.env().emit_event(Liquidate { liquidator: caller, borrower, amount });
            
            Ok(())
        }

        /// Accrue interest on total borrowed amount
        #[ink(message)]
        pub fn accrue_interest(&mut self) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            // Calculate and add interest
            let interest = self.calculate_interest();
            self.total_borrow += interest;
            
            // Emit interest accrued event
            self.env().emit_event(InterestAccrued { amount: interest });
            
            Ok(())
        }

        /// Update the interest rate model
        #[ink(message)]
        pub fn set_interest_rate_model(&mut self, new_model: AccountId) -> Result<(), Error> {
            // Only admin can update interest rate model
            self.only_admin()?;
            
            // Update interest rate model
            self.interest_rate_model = new_model;
            
            // Emit interest rate model update event
            self.env().emit_event(InterestRateModelUpdated { new_model });
            
            Ok(())
        }

        /// Add collateral for a user
        #[ink(message)]
        pub fn add_collateral(&mut self, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let collateral = self.collaterals.get(&caller).unwrap_or(0);
            
            // Update user collateral
            self.collaterals.insert(&caller, &(collateral + amount));
            
            // Emit collateral added event
            self.env().emit_event(CollateralAdded { user: caller, amount });
            
            Ok(())
        }

        /// Remove collateral for a user
        #[ink(message)]
        pub fn remove_collateral(&mut self, amount: Balance) -> Result<(), Error> {
            // Check if contract is not paused
            self.not_paused()?;
            
            let caller = self.env().caller();
            let collateral = self.collaterals.get(&caller).unwrap_or(0);
            
            // Check for sufficient collateral
            if collateral < amount {
                return Err(Error::InsufficientCollateral);
            }
            
            // Update user collateral
            self.collaterals.insert(&caller, &(collateral - amount));
            
            // Emit collateral removed event
            self.env().emit_event(CollateralRemoved { user: caller, amount });
            
            Ok(())
        }

        /// Get account liquidity (difference between collateral and debt)
        #[ink(message)]
        pub fn get_account_liquidity(&self, user: AccountId) -> Balance {
            let collateral = self.collaterals.get(&user).unwrap_or(0);
            let debt = self.debts.get(&user).unwrap_or(0);
            
            // Saturating subtraction ensures no negative values
            collateral.saturating_sub(debt)
        }

        /// Get total assets supplied to the protocol
        #[ink(message)]
        pub fn get_total_supply(&self) -> Balance {
            self.total_supply
        }

        /// Get total assets borrowed from the protocol
        #[ink(message)]
        pub fn get_total_borrow(&self) -> Balance {
            self.total_borrow
        }

        /// Pause the entire protocol
        #[ink(message)]
        pub fn pause_contract(&mut self) -> Result<(), Error> {
            // Only admin can pause
            self.only_admin()?;
            
            self.paused = true;
            
            // Emit contract paused event
            self.env().emit_event(ContractPaused);
            
            Ok(())
        }

        /// Unpause the protocol
        #[ink(message)]
        pub fn unpause_contract(&mut self) -> Result<(), Error> {
            // Only admin can unpause
            self.only_admin()?;
            
            self.paused = false;
            
            // Emit contract unpaused event
            self.env().emit_event(ContractUnpaused);
            
            Ok(())
        }

        /// Internal function to check admin authorization
        fn only_admin(&self) -> Result<(), Error> {
            if self.env().caller() != self.admin {
                return Err(Error::NotAuthorized);
            }
            Ok(())
        }

        /// Internal function to check if contract is not paused
        fn not_paused(&self) -> Result<(), Error> {
            if self.paused {
                return Err(Error::ContractPaused);
            }
            Ok(())
        }

        /// Calculate maximum borrowable amount based on collateral
        fn calculate_max_borrow(&self, collateral: Balance) -> Balance {
            // Simple logic: allow borrowing up to 50% of collateral
            collateral / 2
        }

        /// Calculate interest accrued
        fn calculate_interest(&self) -> Balance {
            // Simple interest calculation: 1% of total borrow
            self.total_borrow / 100
        } 
    }
}

#[cfg(test)]
mod tests {
    // Optional test module
}