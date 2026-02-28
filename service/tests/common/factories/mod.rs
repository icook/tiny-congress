//! Test data factories for reducing test setup boilerplate.
//!
//! # Usage
//!
//! ```rust
//! use common::factories::{AccountFactory, TestItemFactory};
//!
//! let mut tx = test_transaction().await;
//! let account = AccountFactory::new().with_username("alice").create(&mut *tx).await;
//! let item = TestItemFactory::new().with_name("test item").create(&mut *tx).await;
//! ```

mod account;
mod signup;
mod test_item;

pub use account::{generate_test_keys, AccountFactory};
pub use signup::{valid_signup_json, valid_signup_with_keys, SignupKeys};
pub use test_item::TestItemFactory;

use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for generating unique test data.
/// Each call to `next_id()` returns a unique value across all tests.
static FACTORY_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns a unique ID for generating test data.
/// Thread-safe and guaranteed unique within a test run.
pub fn next_id() -> u64 {
    FACTORY_COUNTER.fetch_add(1, Ordering::SeqCst)
}
