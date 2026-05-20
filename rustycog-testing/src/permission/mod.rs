//! Real-OpenFGA permission fixtures.
//!
//! The wiremock-backed `OpenFgaMockService` was retired in favor of the
//! testcontainer-backed [`crate::common::openfga_testcontainer::TestOpenFga`],
//! which boots a real `openfga/openfga` container and drives the
//! production `OpenFgaPermissionChecker` against the actual Check API.
//!
//! This module re-exports the new fixture at the historical path so a
//! `use rustycog_testing::permission::TestOpenFga;` import continues to
//! resolve. Tests that previously called `OpenFgaFixtures::service()`,
//! `mock_check_allow`, `mock_check_deny`, `mock_check_any`,
//! `mock_check_error`, `mock_check_requires_bearer`, or
//! `verify_check_called` must migrate to the real-tuple API
//! (`TestOpenFga::allow` / `deny` / `read_tuples`); see
//! `Manifesto/tests/component_api_tests.rs` for the canonical pattern.

pub use crate::common::openfga_testcontainer::{
    writable_relation_for, TestOpenFga, TestOpenFgaContainer, TupleKey,
};
