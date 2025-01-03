use std::sync::Once;

// Once https://github.com/d-e-s-o/test-log/issues/35 is fixed we can remove this code.
pub(crate) fn init_logging() {
    static INIT_LOGGING: Once = Once::new();
    INIT_LOGGING.call_once(|| {
        use env_logger;
        let _ = env_logger::builder().is_test(true).try_init();
    });
}
