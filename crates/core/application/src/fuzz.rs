use mivia_domain::path::WinPath;

use crate::launch::parse;

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
