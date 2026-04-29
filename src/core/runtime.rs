pub fn build() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Runtime::new().map_err(|err| format!("Failed to create async runtime: {err}"))
}
