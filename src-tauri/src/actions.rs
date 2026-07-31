use crate::model::{Entity, EntityKind, LaunchTarget};

fn action(id: &str, title: &str, aliases: &[&str], description: &str, target: &str) -> Entity {
    Entity {
        id: format!("action:{id}"),
        kind: EntityKind::SystemAction,
        title: title.to_owned(),
        subtitle: "System Settings".to_owned(),
        aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
        description: description.to_owned(),
        target: LaunchTarget::Url(target.to_owned()),
    }
}

#[cfg(target_os = "macos")]
pub fn system_actions() -> Vec<Entity> {
    vec![
        action(
            "displays",
            "Displays",
            &["display settings", "screen", "monitor", "resolution"],
            "Open display settings to adjust screen brightness, resolution, scaling, color profile, Night Shift, and connected monitors.",
            "x-apple.systempreferences:com.apple.Displays-Settings.extension",
        ),
        action(
            "wifi",
            "Wi-Fi",
            &["wireless", "internet", "network", "wifi settings"],
            "Open Wi-Fi settings to connect to wireless networks and manage network details.",
            "x-apple.systempreferences:com.apple.wifi-settings-extension",
        ),
        action(
            "bluetooth",
            "Bluetooth",
            &["wireless devices", "headphones", "keyboard", "mouse"],
            "Open Bluetooth settings to connect and manage wireless accessories and devices.",
            "x-apple.systempreferences:com.apple.BluetoothSettings",
        ),
        action(
            "sound",
            "Sound",
            &["volume", "speakers", "microphone", "audio"],
            "Open sound settings to adjust volume, speakers, microphones, alert sounds, input, and output devices.",
            "x-apple.systempreferences:com.apple.Sound-Settings.extension",
        ),
        action(
            "appearance",
            "Appearance",
            &["dark mode", "light mode", "theme", "accent color"],
            "Open appearance settings to change dark mode, light mode, colors, and interface styling.",
            "x-apple.systempreferences:com.apple.Appearance-Settings.extension",
        ),
        action(
            "keyboard",
            "Keyboard",
            &[
                "keyboard settings",
                "shortcuts",
                "key repeat",
                "input source",
            ],
            "Open keyboard settings to configure shortcuts, input sources, function keys, and key repeat.",
            "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
        ),
        action(
            "battery",
            "Battery",
            &["power", "energy", "low power mode", "charging"],
            "Open battery settings to inspect power usage, charging, and low power mode.",
            "x-apple.systempreferences:com.apple.Battery-Settings.extension",
        ),
        action(
            "notifications",
            "Notifications",
            &["alerts", "banners", "do not disturb", "focus"],
            "Open notification settings to control alerts, banners, sounds, and app notifications.",
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
        ),
    ]
}

#[cfg(target_os = "windows")]
pub fn system_actions() -> Vec<Entity> {
    vec![
        action(
            "displays",
            "Display",
            &["display settings", "screen", "monitor", "resolution"],
            "Open display settings to adjust screen brightness, resolution, scaling, and connected monitors.",
            "ms-settings:display",
        ),
        action(
            "wifi",
            "Wi-Fi",
            &["wireless", "internet", "network", "wifi settings"],
            "Open Wi-Fi settings to connect to wireless networks and manage network details.",
            "ms-settings:network-wifi",
        ),
        action(
            "bluetooth",
            "Bluetooth & devices",
            &[
                "bluetooth",
                "wireless devices",
                "headphones",
                "keyboard",
                "mouse",
            ],
            "Open Bluetooth settings to connect and manage wireless accessories and devices.",
            "ms-settings:bluetooth",
        ),
        action(
            "sound",
            "Sound",
            &["volume", "speakers", "microphone", "audio"],
            "Open sound settings to adjust volume, speakers, microphones, input, and output devices.",
            "ms-settings:sound",
        ),
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn system_actions() -> Vec<Entity> {
    Vec::new()
}
