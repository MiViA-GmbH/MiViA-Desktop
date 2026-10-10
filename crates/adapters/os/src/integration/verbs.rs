use std::io;
use std::path::Path;

use super::user;

pub(super) fn remove_verb(path: &str, ours: impl Fn(&str) -> bool) -> io::Result<bool> {
    let Ok(key) = user().open_subkey(path) else {
        return Ok(false);
    };
    let command: Option<String> = key
        .open_subkey("command")
        .ok()
        .and_then(|command| command.get_value("").ok());
    if command.is_some_and(|command| !ours(&command)) {
        return Ok(false);
    }
    drop(key);
    user().delete_subkey_all(path)?;
    let parents = Path::new(path).ancestors().skip(1).take(2);
    for parent in parents {
        let empty = user().open_subkey(parent).is_ok_and(|key| {
            key.enum_keys().next().is_none() && key.enum_values().next().is_none()
        });
        if empty {
            drop(user().delete_subkey(parent));
        }
    }
    Ok(true)
}
