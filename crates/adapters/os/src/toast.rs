use std::io;

use parking_lot::Mutex;
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    NotificationSetting, ToastNotification, ToastNotificationManager,
};
use windows_core::{HSTRING, h};

pub const APP_ID: &str = "ai.mivia.desktop";
const GROUP: &str = "update";

static SHOWN: Mutex<Vec<ToastNotification>> = Mutex::new(Vec::new());

pub struct Toast<'a> {
    pub tag: &'a str,
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub button: Option<&'a str>,
}

fn document(toast: &Toast<'_>) -> windows_core::Result<XmlDocument> {
    let document = XmlDocument::new()?;
    let root = document.CreateElement(h!("toast"))?;
    let visual = document.CreateElement(h!("visual"))?;
    let binding = document.CreateElement(h!("binding"))?;
    binding.SetAttribute(h!("template"), h!("ToastGeneric"))?;
    for line in std::iter::once(toast.title).chain(toast.body) {
        let text = document.CreateElement(h!("text"))?;
        text.SetInnerText(&HSTRING::from(line))?;
        binding.AppendChild(&text)?;
    }
    visual.AppendChild(&binding)?;
    root.AppendChild(&visual)?;
    if let Some(label) = toast.button {
        let actions = document.CreateElement(h!("actions"))?;
        let action = document.CreateElement(h!("action"))?;
        action.SetAttribute(h!("content"), &HSTRING::from(label))?;
        action.SetAttribute(h!("arguments"), h!("button"))?;
        actions.AppendChild(&action)?;
        root.AppendChild(&actions)?;
    }
    document.AppendChild(&root)?;
    Ok(document)
}

fn notification(toast: &Toast<'_>) -> windows_core::Result<ToastNotification> {
    let notification = ToastNotification::CreateToastNotification(&document(toast)?)?;
    notification.SetTag(&HSTRING::from(toast.tag))?;
    notification.SetGroup(&HSTRING::from(GROUP))?;
    Ok(notification)
}

pub fn show_toast(toast: &Toast<'_>, on_click: impl Fn() + Send + 'static) -> io::Result<bool> {
    let shown = || -> windows_core::Result<bool> {
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?;
        if notifier
            .Setting()
            .is_ok_and(|setting| setting != NotificationSetting::Enabled)
        {
            return Ok(false);
        }
        let notification = notification(toast)?;
        notification.Activated(&TypedEventHandler::new(move |_, _| {
            on_click();
            Ok(())
        }))?;
        notifier.Show(&notification)?;
        SHOWN.lock().push(notification);
        Ok(true)
    };
    shown().map_err(io::Error::other)
}

pub fn withdraw_toasts() -> io::Result<()> {
    SHOWN.lock().clear();
    ToastNotificationManager::History()
        .and_then(|history| {
            history.RemoveGroupWithId(&HSTRING::from(GROUP), &HSTRING::from(APP_ID))
        })
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::{Toast, notification};

    #[test]
    fn text_is_escaped_and_the_toast_carries_tag_group_and_button() {
        let toast = Toast {
            tag: "2.1.0",
            title: "Update <available> & \"ready\"",
            body: Some("Version 2.1.0 is ready to install."),
            button: Some("Release notes"),
        };
        let built = notification(&toast).unwrap();
        assert_eq!(built.Tag().unwrap(), "2.1.0");
        assert_eq!(built.Group().unwrap(), "update");
        let xml = built.Content().unwrap().GetXml().unwrap().to_string();
        assert_eq!(
            xml,
            "<toast><visual><binding template=\"ToastGeneric\">\
             <text>Update &lt;available&gt; &amp; \"ready\"</text>\
             <text>Version 2.1.0 is ready to install.</text></binding></visual>\
             <actions><action content=\"Release notes\" arguments=\"button\"/></actions></toast>"
        );
    }

    #[test]
    fn a_title_only_toast_has_one_text_and_no_actions() {
        let toast = Toast {
            tag: "2.1.0",
            title: "Updated to 2.1.0.",
            body: None,
            button: None,
        };
        let xml = notification(&toast)
            .unwrap()
            .Content()
            .unwrap()
            .GetXml()
            .unwrap()
            .to_string();
        assert_eq!(xml.matches("<text>").count(), 1);
        assert!(!xml.contains("actions"));
    }
}
