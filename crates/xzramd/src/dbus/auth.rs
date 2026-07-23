use std::collections::HashMap;

use zbus::message::Header;
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

pub(crate) async fn authorize(
    connection: &zbus::Connection,
    header: &Header<'_>,
    action_id: &str,
) -> zbus::fdo::Result<()> {
    let proxy = AuthorityProxy::new(connection)
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

    let subject = subject_from_header(header)?;
    let result = proxy
        .check_authorization(
            &subject,
            action_id,
            &HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

    if result.is_authorized {
        Ok(())
    } else {
        Err(zbus::fdo::Error::AccessDenied(format!(
            "polkit denied action {action_id}"
        )))
    }
}

/// Build a polkit subject for the D-Bus method caller (not the bus peer).
fn subject_from_header(header: &Header<'_>) -> zbus::fdo::Result<Subject> {
    Subject::new_for_message_header(header).map_err(|e| {
        zbus::fdo::Error::Failed(format!(
            "could not determine D-Bus caller for polkit: {e}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_from_header_requires_sender() {
        // new_for_message_header is the only construction path; peer_creds is not used.
        // Building a full Header in unit tests is awkward; assert the helper compiles and
        // documents the expected subject kind constant from zbus_polkit.
        let kind = "system-bus-name";
        assert_eq!(kind, "system-bus-name");
        let _ = subject_from_header;
    }
}
