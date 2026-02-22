mod chat_modal;
mod messaging;

pub use chat_modal::ChatModal;
pub use messaging::{
    ChatMessage, GlobalLanMessaging, LanMessagingEntity, LanMessagingEvent, UserIdentity,
};

use gpui::App;

pub fn init(cx: &mut App) {
    LanMessagingEntity::init(cx);
}
