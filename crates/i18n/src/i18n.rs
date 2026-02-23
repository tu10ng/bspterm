use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, SharedString};

rust_i18n::i18n!("locales", fallback = "en");

pub fn t(key: &str) -> SharedString {
    let locale = rust_i18n::locale();
    SharedString::from(_rust_i18n_translate(&locale, key).to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Locale {
    English,
    ChineseSimplified,
}

impl Locale {
    pub fn code(&self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::ChineseSimplified => "zh-CN",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::ChineseSimplified => "简体中文",
        }
    }

    pub fn all() -> &'static [Locale] {
        &[Locale::ChineseSimplified, Locale::English]
    }
}

#[derive(Clone, Debug)]
pub enum LocaleEvent {
    Changed(Locale),
}

pub struct GlobalLocale(pub Entity<LocaleEntity>);
impl Global for GlobalLocale {}

pub struct LocaleEntity {
    current: Locale,
}

impl EventEmitter<LocaleEvent> for LocaleEntity {}

impl LocaleEntity {
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalLocale>().is_some() {
            return;
        }
        let locale = Locale::ChineseSimplified;
        rust_i18n::set_locale(locale.code());
        let entity = cx.new(|_| Self { current: locale });
        cx.set_global(GlobalLocale(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalLocale>().0.clone()
    }

    pub fn current(&self) -> &Locale {
        &self.current
    }

    pub fn set_locale(&mut self, locale: Locale, cx: &mut Context<Self>) {
        if self.current != locale {
            self.current = locale.clone();
            rust_i18n::set_locale(locale.code());
            cx.emit(LocaleEvent::Changed(locale));
            cx.notify();
        }
    }
}
