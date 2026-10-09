//! Desktop notifications without extra dependencies: PowerShell toast on
//! Windows, `notify-send` on Linux, `osascript` on macOS. Text is passed through
//! environment variables so it is never interpreted as code.

use std::process::{Command, Stdio};

pub fn show(title: &str, body: &str) {
    let title: String = title.chars().take(80).collect();
    let body: String = body.chars().take(200).collect();
    let mut cmd = build(&title, &body);
    cmd.env("N_TITLE", &title)
        .env("N_BODY", &body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Fire and forget; a missing notifier is not an error worth surfacing.
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(target_os = "windows")]
fn build(_title: &str, _body: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] > $null
$xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$nodes = $xml.GetElementsByTagName('text')
$nodes.Item(0).AppendChild($xml.CreateTextNode($env:N_TITLE)) > $null
$nodes.Item(1).AppendChild($xml.CreateTextNode($env:N_BODY)) > $null
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe').Show($toast)
"#;
    let mut c = Command::new("powershell");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ])
    .creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(target_os = "macos")]
fn build(_title: &str, _body: &str) -> Command {
    let mut c = Command::new("osascript");
    c.args([
        "-e",
        "display notification (system attribute \"N_BODY\") with title (system attribute \"N_TITLE\")",
    ]);
    c
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn build(title: &str, body: &str) -> Command {
    let mut c = Command::new("notify-send");
    c.args(["--app-name=Discord", "--", title, body]);
    c
}
