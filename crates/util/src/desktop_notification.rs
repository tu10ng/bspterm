use anyhow::Result;

/// A desktop notification that can be displayed to the user.
pub struct DesktopNotification {
    pub title: String,
    pub body: String,
}

impl DesktopNotification {
    /// Create a new desktop notification.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
        }
    }

    /// Send the notification to the desktop.
    pub async fn send(self) -> Result<()> {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            self.send_linux().await
        }

        #[cfg(target_os = "windows")]
        {
            self.send_windows().await
        }

        #[cfg(target_os = "macos")]
        {
            self.send_macos().await
        }

        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "windows",
            target_os = "macos"
        )))]
        {
            log::warn!("Desktop notifications not supported on this platform");
            Ok(())
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    async fn send_linux(self) -> Result<()> {
        use ashpd::desktop::notification::{Notification, NotificationProxy, Priority};

        log::info!(
            "[NOTIFICATION] Sending Linux notification: title=\"{}\", body=\"{}\"",
            self.title,
            self.body
        );

        let proxy = NotificationProxy::new().await?;
        let notification_id = format!("bspterm-{}", uuid::Uuid::new_v4());

        proxy
            .add_notification(
                &notification_id,
                Notification::new(&self.title)
                    .body(Some(self.body.as_str()))
                    .priority(Priority::Normal),
            )
            .await?;

        log::debug!("[NOTIFICATION] Notification sent successfully (id={})", notification_id);

        Ok(())
    }

    #[cfg(target_os = "windows")]
    async fn send_windows(self) -> Result<()> {
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

        let toast_xml = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastGeneric">
                        <text>{}</text>
                        <text>{}</text>
                    </binding>
                </visual>
            </toast>"#,
            xml_escape(&self.title),
            xml_escape(&self.body)
        );

        let doc = XmlDocument::new()?;
        doc.LoadXml(&windows::core::HSTRING::from(&toast_xml))?;

        let toast = ToastNotification::CreateToastNotification(&doc)?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(
            &windows::core::HSTRING::from("bspterm"),
        )?;

        notifier.Show(&toast)?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    async fn send_macos(self) -> Result<()> {
        use std::process::Command;

        let script = format!(
            r#"display notification "{}" with title "{}""#,
            applescript_escape(&self.body),
            applescript_escape(&self.title)
        );

        Command::new("osascript").args(["-e", &script]).status()?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
