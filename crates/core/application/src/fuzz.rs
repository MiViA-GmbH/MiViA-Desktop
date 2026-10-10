use mivia_domain::path::WinPath;
use mivia_domain::sanitise::is_hidden_format;

use crate::launch::parse;
use crate::watch_marker::MAX_FIELD_CHARS;
use crate::{legacy, spool, watch_marker};

pub fn launch_args(data: &[u8]) -> Result<(), String> {
    let text = String::from_utf8_lossy(data);
    let args: Vec<String> = text.split('\0').map(str::to_owned).collect();
    let launch = parse(&args);
    let flags = usize::from(launch.minimized) + usize::from(launch.open_paths);
    let stable = launch
        .paths
        .iter()
        .all(|path| WinPath::parse(&path.to_string()).as_ref() == Ok(path));
    let accounted = launch.paths.len() + launch.ignored.len() + flags <= args.len();
    let paths_need_flag = launch.open_paths || launch.paths.is_empty();
    match (stable, accounted, paths_need_flag) {
        (true, true, true) => Ok(()),
        (false, _, _) => Err(format!("path does not round trip: {args:?}")),
        (_, false, _) => Err(format!("arguments not accounted for: {args:?}")),
        (_, _, false) => Err(format!("paths without --open-paths: {args:?}")),
    }
}

pub fn spool_file(data: &[u8]) -> Result<(), String> {
    let Ok(parsed) = spool::parse(data) else {
        return Ok(());
    };
    let encoded = spool::encode(parsed.pid, &parsed.paths, parsed.non_unicode)
        .map_err(|error| format!("encode failed: {error}"))?;
    let again = spool::parse(&encoded).map_err(|error| format!("reparse failed: {error:?}"))?;
    match (again.paths == parsed.paths, again.rejected == 0) {
        (true, true) => Ok(()),
        (false, _) => Err(format!("paths do not round trip: {:?}", parsed.paths)),
        (_, false) => Err(format!("encoded spool rejects paths: {:?}", parsed.paths)),
    }
}

pub fn watch_marker(data: &[u8]) -> Result<(), String> {
    let Ok(marker) = watch_marker::parse(data) else {
        return Ok(());
    };
    let fields = [
        &marker.machine_name,
        &marker.windows_user,
        &marker.app_version,
    ];
    let capped = fields
        .iter()
        .all(|field| field.chars().count() <= MAX_FIELD_CHARS);
    let visible = fields
        .iter()
        .all(|field| !field.chars().any(is_hidden_format));
    match (capped, visible) {
        (true, true) => Ok(()),
        (false, _) => Err(format!("field over the cap: {marker:?}")),
        (_, false) => Err(format!("field keeps hidden characters: {marker:?}")),
    }
}

pub fn legacy_config(bytes: &[u8]) -> Result<(), String> {
    let Ok(config) = legacy::parse(bytes) else {
        return Ok(());
    };
    let blanked = legacy::blank_access_token(bytes).map_err(|error| format!("{error:?}"))?;
    let again = legacy::parse(&blanked).map_err(|error| format!("reparse {error:?}"))?;
    if again.access_token.is_some() {
        return Err("token survived".to_owned());
    }
    let kept = legacy::LegacyConfig {
        access_token: None,
        ..config
    };
    (again == kept)
        .then_some(())
        .ok_or_else(|| "settings changed".to_owned())
}
