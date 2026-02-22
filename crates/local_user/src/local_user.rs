mod login_modal;
mod store;

pub use login_modal::LocalLoginModal;
pub use store::{
    GlobalLocalUserStore, LocalUserProfile, LocalUserStore, LocalUserStoreEntity,
    LocalUserStoreEvent, NetworkInterface,
};

use gpui::App;
use workspace::Workspace;

pub fn init(cx: &mut App) {
    LocalUserStoreEntity::init(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(
            |workspace, _: &bspterm_actions::local_user::SignIn, window, cx| {
                workspace.toggle_modal(window, cx, |window, cx| LocalLoginModal::new(window, cx));
            },
        );

        workspace.register_action(
            |_workspace, _: &bspterm_actions::local_user::SignOut, _window, cx| {
                if let Some(store) = LocalUserStoreEntity::try_global(cx) {
                    store.update(cx, |store, cx| {
                        store.clear_profile(cx);
                    });
                }
            },
        );
    })
    .detach();
}
