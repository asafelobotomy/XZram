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
            CheckAuthorizationFlags::ALLOW_USER_INTERACTION,
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
        zbus::fdo::Error::Failed(format!("could not determine D-Bus caller for polkit: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(sender: Option<&str>) -> zbus::Message {
        let builder = zbus::Message::method_call("/io/github/XZram1", "GetStatus");
        let builder = match sender {
            Some(name) => builder.sender(name),
            None => builder,
        };
        builder.build(&()).unwrap()
    }

    #[test]
    fn subject_is_the_method_caller() {
        let msg = call(Some(":1.42"));
        let subject = subject_from_header(&msg.header()).expect("subject for a sender");
        // polkit must check the bus name that sent this call, nothing else.
        assert!(format!("{subject:?}").contains(":1.42"), "{subject:?}");
    }

    #[test]
    fn subject_from_header_requires_sender() {
        let msg = call(None);
        match subject_from_header(&msg.header()) {
            Err(zbus::fdo::Error::Failed(text)) => {
                assert!(text.contains("could not determine D-Bus caller"), "{text}")
            }
            other => panic!("expected Failed without a sender, got {other:?}"),
        }
    }
}
