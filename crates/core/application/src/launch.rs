use mivia_domain::path::WinPath;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub minimized: bool,
    pub open_paths: bool,
    pub paths: Vec<WinPath>,
    pub ignored: Vec<String>,
}

pub fn parse(args: &[String]) -> Launch {
    let mut launch = Launch::default();
    let mut rest = args.iter();
    for arg in rest.by_ref() {
        match arg.as_str() {
            "--" => break,
            "--minimized" => launch.minimized = true,
            "--open-paths" => launch.open_paths = true,
            _ => launch.ignored.push(arg.clone()),
        }
    }
    for arg in rest {
        match path(arg).filter(|_| launch.open_paths) {
            Some(path) => launch.paths.push(path),
            None => launch.ignored.push(arg.clone()),
        }
    }
    launch
}

fn path(arg: &str) -> Option<WinPath> {
    let restored = arg
        .strip_suffix('"')
        .map_or_else(|| arg.to_owned(), |head| format!("{head}\\"));
    WinPath::parse(&restored).ok()
}
