mod broadcast;
mod discovery;

pub use broadcast::{ActiveSessionInfo, SessionProtocol, UserPresenceBroadcast};
pub use discovery::{
    DiscoveredUser, GlobalLanDiscovery, LanDiscoveryEntity, LanDiscoveryEvent,
};

use gpui::App;

pub fn init(cx: &mut App) {
    LanDiscoveryEntity::init(cx);
}
