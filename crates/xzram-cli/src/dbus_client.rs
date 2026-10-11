use std::fmt;

use zbus::blocking::{Connection, Proxy};
use zbus::message::Message;

const SERVICE: &str = "io.github.XZram1";
const OBJECT_PATH: &str = "/io/github/XZram";
const INTERFACE: &str = "io.github.XZram.Manager";

/// xzramd could not be reached *before* the request was delivered (bus connect failed,
/// name has no owner, activation failed). Retrying via pkexec is safe.
#[derive(Debug)]
pub struct ServiceUnavailable(pub String);

impl fmt::Display for ServiceUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xzramd unavailable: {}", self.0)
    }
}

impl std::error::Error for ServiceUnavailable {}

/// How a failed D-Bus method call should be treated.
#[derive(Debug, PartialEq, Eq)]
enum CallFailure {
    /// The request never reached xzramd.
    Unavailable,
    /// The request may have been executed; the reply was lost.
    OutcomeUnknown,
    /// xzramd (or the bus) replied with a definite error.
    Failed,
}

fn classify_error_name(name: &str) -> CallFailure {
    match name {
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => CallFailure::Unavailable,
        n if n.starts_with("org.freedesktop.DBus.Error.Spawn.") => CallFailure::Unavailable,
        "org.freedesktop.DBus.Error.NoReply"
        | "org.freedesktop.DBus.Error.Disconnected"
        | "org.freedesktop.DBus.Error.Timeout"
        | "org.freedesktop.DBus.Error.TimedOut" => CallFailure::OutcomeUnknown,
        _ => CallFailure::Failed,
    }
}

fn classify(err: &zbus::Error) -> CallFailure {
    use zbus::DBusError;
    match err {
        zbus::Error::MethodError(name, _, _) => classify_error_name(name.as_str()),
        zbus::Error::FDO(e) => classify_error_name(e.name().as_str()),
        // The connection broke after the request was (possibly) sent.
        zbus::Error::InputOutput(_) => CallFailure::OutcomeUnknown,
        _ => CallFailure::Failed,
    }
}

/// Call a Manager method, mapping transport failures to `ServiceUnavailable` (safe to
/// retry) or an "outcome unknown" error (must never be retried automatically).
pub fn call_classified<B>(proxy: &Proxy<'_>, method: &str, body: &B) -> anyhow::Result<Message>
where
    B: serde::ser::Serialize + zbus::zvariant::DynamicType,
{
    proxy
        .call_method(method, body)
        .map_err(|e| match classify(&e) {
            CallFailure::Unavailable => ServiceUnavailable(e.to_string()).into(),
            CallFailure::OutcomeUnknown => anyhow::anyhow!(
                "xzramd stopped responding during {method} ({e}); the operation may or may not \
             have been applied — check `xzram status` before retrying"
            ),
            CallFailure::Failed => anyhow::Error::new(e),
        })
}

/// Connect to the system bus and build a Manager proxy. Failures here happen before any
/// request is sent, so they are reported as `ServiceUnavailable`.
pub fn manager_proxy() -> anyhow::Result<(Connection, Proxy<'static>)> {
    let conn = Connection::system().map_err(|e| ServiceUnavailable(e.to_string()))?;
    let proxy = Proxy::new(&conn, SERVICE, OBJECT_PATH, INTERFACE)
        .map_err(|e| ServiceUnavailable(e.to_string()))?;
    Ok((conn, proxy))
}

pub fn is_available() -> bool {
    Connection::system()
        .and_then(|conn| {
            conn.call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &(SERVICE,),
            )
            .and_then(|reply| reply.body().deserialize::<bool>())
        })
        .unwrap_or(false)
}

/// `keep` from a `snapshot.prune` payload, validated to fit the D-Bus `u32` argument.
fn prune_keep(payload: &serde_json::Value) -> anyhow::Result<u32> {
    let Some(keep) = payload.get("keep") else {
        return Ok(crate::args::DEFAULT_PRUNE_KEEP);
    };
    let keep = keep
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("snapshot.prune: keep must be a non-negative integer"))?;
    u32::try_from(keep).map_err(|_| anyhow::anyhow!("snapshot.prune: keep {keep} is too large"))
}

fn print_messages(reply: &Message) -> anyhow::Result<()> {
    let messages: Vec<String> = reply.body().deserialize()?;
    for msg in messages {
        println!("{msg}");
    }
    Ok(())
}

fn print_json_field(reply: &Message) -> anyhow::Result<()> {
    let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
        reply.body().deserialize()?;
    if let Some(json) = map.get("json") {
        if let Ok(zbus::zvariant::Value::Str(json_str)) = json.downcast_ref() {
            println!("{json_str}");
        }
    }
    Ok(())
}

pub fn call(action: &str, payload: &str) -> anyhow::Result<()> {
    let (_conn, proxy) = manager_proxy()?;
    let call = |method: &str| call_classified(&proxy, method, &());

    match action {
        "stage" => {
            call_classified(&proxy, "StageAction", &(payload,))?;
        }
        "pending.clear" => {
            call("ClearPending")?;
        }
        "apply" => print_messages(&call("Apply")?)?,
        "rollback" => print_messages(&call("Rollback")?)?,
        "snapshot.restore" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let id = parsed["id"].as_str().unwrap_or_default();
            print_messages(&call_classified(&proxy, "RestoreSnapshot", &(id,))?)?;
        }
        "snapshot.delete" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let id = parsed["id"].as_str().unwrap_or_default();
            call_classified(&proxy, "DeleteSnapshot", &(id,))?;
            println!("Deleted snapshot {id}");
        }
        "snapshot.prune" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let keep = prune_keep(&parsed)?;
            let reply = call_classified(&proxy, "PruneSnapshots", &(keep,))?;
            let removed: u32 = reply.body().deserialize()?;
            println!("Pruned {removed} snapshot(s)");
        }
        "snapshot.create" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let trigger = parsed["trigger"].as_str().unwrap_or("manual");
            let label = parsed["label"].as_str().unwrap_or("");
            print_json_field(&call_classified(
                &proxy,
                "CreateSnapshot",
                &(trigger, label),
            )?)?;
        }
        "zram.configure" => {
            call_classified(&proxy, "ApplyNowZram", &(payload,))?;
            println!("ZRAM configured");
        }
        "zram.disable" => {
            call("ApplyNowZramDisable")?;
            println!("ZRAM disabled");
        }
        "swapfile.create" => {
            call_classified(&proxy, "ApplyNowSwapfileCreate", &(payload,))?;
            println!("Swapfile created");
        }
        "swapfile.resize" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let path = parsed["path"].as_str().unwrap_or_default();
            let size_mb = parsed["size_mb"].as_u64().unwrap_or(0);
            call_classified(&proxy, "ApplyNowSwapfileResize", &(path, size_mb))?;
            println!("Swapfile resized");
        }
        "swapfile.remove" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let path = parsed["path"].as_str().unwrap_or_default();
            call_classified(&proxy, "ApplyNowSwapfileRemove", &(path,))?;
            println!("Swapfile removed");
        }
        "swapfile.prepare" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let path = parsed["path"].as_str().unwrap_or_default();
            let mkdir_parents = parsed["mkdir_parents"].as_bool().unwrap_or(false);
            print_json_field(&call_classified(
                &proxy,
                "PrepareSwapfileBtrfs",
                &(path, mkdir_parents),
            )?)?;
        }
        "sysctl.set" => {
            call_classified(&proxy, "ApplyNowSysctl", &(payload,))?;
            println!("Sysctl values applied");
        }
        "zram.migrate" => {
            call("MigrateZram")?;
        }
        "swap.activate" => {
            // Rejected before any request is sent, so pkexec fallback is safe.
            return Err(ServiceUnavailable("swap.activate not supported via D-Bus".into()).into());
        }
        other => anyhow::bail!("unsupported D-Bus action: {other}"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_pre_call_failures_as_unavailable() {
        for name in [
            "org.freedesktop.DBus.Error.ServiceUnknown",
            "org.freedesktop.DBus.Error.NameHasNoOwner",
            "org.freedesktop.DBus.Error.Spawn.ChildExited",
        ] {
            assert_eq!(
                classify_error_name(name),
                CallFailure::Unavailable,
                "{name}"
            );
        }
    }

    #[test]
    fn classifies_lost_replies_as_outcome_unknown() {
        for name in [
            "org.freedesktop.DBus.Error.NoReply",
            "org.freedesktop.DBus.Error.Disconnected",
        ] {
            assert_eq!(
                classify_error_name(name),
                CallFailure::OutcomeUnknown,
                "{name}"
            );
        }
    }

    #[test]
    fn daemon_errors_are_plain_failures() {
        assert_eq!(
            classify_error_name("org.freedesktop.DBus.Error.AccessDenied"),
            CallFailure::Failed
        );
        assert_eq!(
            classify_error_name("io.github.XZram.Error.Failed"),
            CallFailure::Failed
        );
    }

    #[test]
    fn prune_keep_rejects_values_beyond_u32() {
        let err = prune_keep(&serde_json::json!({ "keep": 4_294_967_297_u64 })).unwrap_err();
        assert!(err.to_string().contains("too large"));
        assert_eq!(prune_keep(&serde_json::json!({ "keep": 7 })).unwrap(), 7);
        assert_eq!(prune_keep(&serde_json::json!({})).unwrap(), 50);
        assert!(prune_keep(&serde_json::json!({ "keep": -1 })).is_err());
    }
}
