//! The ticket's explicit browser action, never a resource from its body
//! (`PRD-07`, `PRD-10`). macOS opens it; the webview never loads the tracker.

use std::process::Stdio;
use std::time::Duration;

use crate::contract::BridgeError;

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::Invalid {
        message: message.into(),
    }
}

fn browser_command(url: &str) -> Result<tokio::process::Command, BridgeError> {
    let url = tauri::Url::parse(url).map_err(|_| invalid("This ticket has no web address."))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid(
            "Only http and https ticket addresses can be opened.",
        ));
    }
    let mut command = tokio::process::Command::new("/usr/bin/open");
    command
        .args(["--", url.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    Ok(command)
}

#[tauri::command]
pub(crate) async fn open_ticket_url(url: String) -> Result<(), BridgeError> {
    let mut command = browser_command(&url)?;
    let status = tokio::time::timeout(Duration::from_secs(5), command.status())
        .await
        .map_err(|_| invalid("The browser did not open in time. Try again."))?
        .map_err(|error| invalid(format!("The browser could not be opened: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(invalid("The browser could not be opened. Try again."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prd_07_prd_10_only_web_addresses_reach_the_browser() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "-a Terminal",
            "not a URL",
        ] {
            assert!(browser_command(url).is_err(), "{url}");
        }
    }

    #[test]
    fn arc_24_a_ticket_url_is_one_argument_never_shell_code() -> Result<(), BridgeError> {
        let command = browser_command("https://example.test/ticket/42?text=$(touch%20/tmp/no)")?;
        let command = command.as_std();
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--");
        assert_eq!(
            args[1],
            "https://example.test/ticket/42?text=$(touch%20/tmp/no)"
        );
        Ok(())
    }
}
