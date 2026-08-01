pub mod executor;

// The executor used to live in `contract::contract`, which shadowed the name
// of its own parent module. Keep the old path resolving so callers (and any
// out-of-tree code) are not broken by the rename.
pub use executor as contract;
