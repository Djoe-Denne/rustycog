//! Compile the real fixture-runtime tests against the normal framework library.
//! This target deliberately avoids unrelated framework `lib test` modules.
#![cfg(feature = "testing")]
#![allow(dead_code)]

#[path = "../rustycog-testing/src/common/fixture_runtime.rs"]
mod fixture_runtime;
