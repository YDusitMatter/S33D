//! Product identity shared by the desktop UI and Windows executable resources.

pub const PRODUCT_NAME: &str = "S33D Alpha Preview";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_ID: &str = match option_env!("S33D_BUILD_ID") {
    Some(id) => id,
    None => "dev",
};
pub const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
pub const LICENSE: &str = env!("CARGO_PKG_LICENSE");

pub const BUILD_TIME: &str = match option_env!("S33D_BUILD_TIME") {
    Some(value) => value,
    None => "development",
};
pub const BUILD_CHANNEL: &str = match option_env!("S33D_BUILD_CHANNEL") {
    Some(value) => value,
    None => "development",
};
pub fn version_report() -> String {
    format!(
        "{}\nVersion: {}\nBuild: {}\nBuilt (UTC): {}\nChannel: {}",
        PRODUCT_NAME, VERSION, BUILD_ID, BUILD_TIME, BUILD_CHANNEL
    )
}
