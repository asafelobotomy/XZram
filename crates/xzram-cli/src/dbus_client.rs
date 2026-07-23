use zbus::blocking::Connection;

pub fn is_available() -> bool {
    Connection::system()
        .and_then(|conn| {
            conn.call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &("io.github.XZram1",),
            )
            .and_then(|reply| reply.body().deserialize::<bool>())
        })
        .unwrap_or(false)
}

pub fn call(action: &str, payload: &str) -> anyhow::Result<()> {
    let conn = Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "io.github.XZram1",
        "/io/github/XZram",
        "io.github.XZram.Manager",
    )?;

    match action {
        "stage" => {
            proxy.call_method("StageAction", &(payload,))?;
        }
        "pending.clear" => {
            proxy.call_method("ClearPending", &())?;
        }
        "apply" => {
            let reply = proxy.call_method("Apply", &())?;
            let messages: Vec<String> = reply.body().deserialize()?;
            for msg in messages {
                println!("{msg}");
            }
        }
        "rollback" => {
            let reply = proxy.call_method("Rollback", &())?;
            let messages: Vec<String> = reply.body().deserialize()?;
            for msg in messages {
                println!("{msg}");
            }
        }
        "snapshot.restore" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let id = parsed["id"].as_str().unwrap_or_default();
            let reply = proxy.call_method("RestoreSnapshot", &(id,))?;
            let messages: Vec<String> = reply.body().deserialize()?;
            for msg in messages {
                println!("{msg}");
            }
        }
        "snapshot.delete" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let id = parsed["id"].as_str().unwrap_or_default();
            proxy.call_method("DeleteSnapshot", &(id,))?;
            println!("Deleted snapshot {id}");
        }
        "snapshot.prune" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let keep = parsed["keep"].as_u64().unwrap_or(50) as usize;
            let reply = proxy.call_method("PruneSnapshots", &(keep as u32,))?;
            let removed: u32 = reply.body().deserialize()?;
            println!("Pruned {removed} snapshot(s)");
        }
        "snapshot.create" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let trigger = parsed["trigger"].as_str().unwrap_or("manual");
            let label = parsed["label"].as_str().unwrap_or("");
            let reply = proxy.call_method("CreateSnapshot", &(trigger, label))?;
            let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
                reply.body().deserialize()?;
            if let Some(json) = map.get("json") {
                if let Ok(zbus::zvariant::Value::Str(json_str)) = json.downcast_ref() {
                    println!("{json_str}");
                }
            }
        }
        "zram.configure" => {
            proxy.call_method("ApplyNowZram", &(payload,))?;
            println!("ZRAM configured");
        }
        "zram.disable" => {
            proxy.call_method("DisableZram", &())?;
            let reply = proxy.call_method("Apply", &())?;
            let messages: Vec<String> = reply.body().deserialize()?;
            for msg in messages {
                println!("{msg}");
            }
            println!("ZRAM disable applied");
        }
        "swapfile.create" => {
            proxy.call_method("ApplyNowSwapfileCreate", &(payload,))?;
            println!("Swapfile created");
        }
        "swapfile.resize" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let path = parsed["path"].as_str().unwrap_or_default();
            let size_mb = parsed["size_mb"].as_u64().unwrap_or(0);
            proxy.call_method("ResizeSwapfile", &(path, size_mb))?;
            proxy.call_method("Apply", &())?;
            println!("Swapfile resized");
        }
        "swapfile.remove" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let path = parsed["path"].as_str().unwrap_or_default();
            proxy.call_method("RemoveSwapfile", &(path,))?;
            proxy.call_method("Apply", &())?;
            println!("Swapfile removed");
        }
        "swapfile.prepare" => {
            let parsed: serde_json::Value = serde_json::from_str(payload)?;
            let path = parsed["path"].as_str().unwrap_or_default();
            let mkdir_parents = parsed["mkdir_parents"].as_bool().unwrap_or(false);
            let reply = proxy.call_method("PrepareSwapfileBtrfs", &(path, mkdir_parents))?;
            let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
                reply.body().deserialize()?;
            if let Some(json) = map.get("json") {
                if let Ok(zbus::zvariant::Value::Str(json_str)) = json.downcast_ref() {
                    println!("{json_str}");
                }
            }
        }
        "sysctl.set" => {
            proxy.call_method("SetSysctl", &(payload,))?;
            proxy.call_method("Apply", &())?;
            println!("Sysctl values applied");
        }
        "zram.migrate" => {
            proxy.call_method("MigrateZram", &())?;
        }
        "swap.activate" => {
            anyhow::bail!("swap.activate not supported via D-Bus; use pkexec");
        }
        other => anyhow::bail!("unsupported D-Bus action: {other}"),
    }

    Ok(())
}
