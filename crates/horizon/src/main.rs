use lib_horizon::init;

/// Yep, that's it.
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    init().await.expect("Failed to initialize Horizon application, an unhandled error occurred.");
}