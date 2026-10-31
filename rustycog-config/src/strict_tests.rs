use super::*;

#[derive(Clone, Default, Serialize, Deserialize)]
struct StrictConfig {
    value: u64,
}
impl ConfigLoader<Self> for StrictConfig {
    fn create_default() -> Self {
        panic!("strict loading must never fall back")
    }
    fn config_prefix() -> &'static str {
        "RUSTYCOG_STRICT_TEST"
    }
}

#[test]
fn load_config_fresh_strict_rejects_bad_type_without_leaking_or_fallback() {
    const KEY: &str = "RUSTYCOG_STRICT_TEST_VALUE";
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.0 {
                Some(value) => std::env::set_var(KEY, value),
                None => std::env::remove_var(KEY),
            }
        }
    }
    let _restore = Restore(std::env::var_os(KEY));
    std::env::set_var(KEY, "sensitive-invalid-type-sentinel");
    let error = load_config_fresh_strict::<StrictConfig>()
        .err()
        .expect("must fail");
    assert!(error.to_string() == "Invalid configuration");
    assert!(!format!("{error:?}").contains("sensitive-invalid-type-sentinel"));
    std::env::set_var(KEY, "42");
    assert!(load_config_fresh_strict::<StrictConfig>().unwrap().value == 42);
}
