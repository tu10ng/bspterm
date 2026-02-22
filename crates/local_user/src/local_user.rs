mod login_modal;
mod store;

pub use login_modal::LocalLoginModal;
pub use store::{
    GlobalLocalUserStore, LocalUserProfile, LocalUserStore, LocalUserStoreEntity,
    LocalUserStoreEvent, NetworkInterface,
};

use gpui::App;

pub fn init(cx: &mut App) {
    LocalUserStoreEntity::init(cx);
}
