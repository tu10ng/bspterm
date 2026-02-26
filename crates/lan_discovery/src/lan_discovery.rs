mod broadcast;
mod central;
mod discovery;

pub use broadcast::{ActiveSessionInfo, SessionProtocol, UserPresenceBroadcast};
pub use discovery::{
    DiscoveredUser, DiscoverySettings, GlobalLanDiscovery, LanDiscoveryEntity, LanDiscoveryEvent,
};

use std::sync::Arc;

use gpui::App;
use http_client::HttpClient;

pub fn init(cx: &mut App, http_client: Arc<dyn HttpClient>) {
    LanDiscoveryEntity::init(cx, http_client);
}
