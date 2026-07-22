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

    let subject = subject_from_header(connection, header).await?;
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

async fn subject_from_header(
    connection: &zbus::Connection,
    _header: &Header<'_>,
) -> zbus::fdo::Result<Subject> {
    let creds = connection
        .peer_creds()
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

    let pid = creds
        .process_id()
        .ok_or_else(|| zbus::fdo::Error::Failed("could not determine caller PID".into()))?;
    let uid = creds.unix_user_id();

    Subject::new_for_owner(pid, None, uid).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
}
