//! Telemetry is disabled in this build.

use futures::channel::mpsc;
pub use telemetry_events::FlexibleEvent as Event;

/// No-op macro that accepts telemetry event calls but does nothing.
/// This allows existing telemetry call sites to compile without modification.
/// The macro consumes all arguments to prevent "unused variable" warnings.
#[macro_export]
macro_rules! event {
    ($name:expr) => {{
        let _ = $name;
    }};
    ($name:expr, $($key:ident $(= $value:expr)?),+ $(,)?) => {{
        let _ = $name;
        $(
            $crate::consume_telemetry_arg!($key $(= $value)?);
        )+
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! consume_telemetry_arg {
    ($key:ident) => {
        let _ = &$key;
    };
    ($key:ident = $value:expr) => {
        let _ = &$value;
    };
}

/// No-op initialization function for compatibility.
pub fn init(_tx: mpsc::UnboundedSender<Event>) {}
