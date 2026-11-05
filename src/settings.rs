use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    TokyoNight,
    Nord,
    Dracula,
    GruvboxDark,
    GruvboxLight,
    CatppuccinMocha,
    CatppuccinLatte,
    SolarizedDark,
    SolarizedLight,
    RosePine,
    FieldJournal,
    OpenCode,
    Aura,
    Ayu,
    Carbonfox,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    Cobalt2,
    Cursor,
    Everforest,
    Flexoki,
    GitHub,
    Kanagawa,
    LucentOrng,
    Material,
    Matrix,
    Mercury,
    Monokai,
    NightOwl,
    OneDark,
    Orng,
    OsakaJade,
    Palenight,
    Synthwave84,
    Vercel,
    Vesper,
    Zenburn,
    System,
}

impl Theme {
    pub const ALL: [Self; 40] = [
        Self::Dark,
        Self::Light,
        Self::TokyoNight,
        Self::Nord,
        Self::Dracula,
        Self::GruvboxDark,
        Self::GruvboxLight,
        Self::CatppuccinMocha,
        Self::CatppuccinLatte,
        Self::SolarizedDark,
        Self::SolarizedLight,
        Self::RosePine,
        Self::FieldJournal,
        Self::OpenCode,
        Self::Aura,
        Self::Ayu,
        Self::Carbonfox,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::Cobalt2,
        Self::Cursor,
        Self::Everforest,
        Self::Flexoki,
        Self::GitHub,
        Self::Kanagawa,
        Self::LucentOrng,
        Self::Material,
        Self::Matrix,
        Self::Mercury,
        Self::Monokai,
        Self::NightOwl,
        Self::OneDark,
        Self::Orng,
        Self::OsakaJade,
        Self::Palenight,
        Self::Synthwave84,
        Self::Vercel,
        Self::Vesper,
        Self::Zenburn,
        Self::System,
    ];

    pub fn label(self) -> &'static str {
        crate::theme::definition(self).name
    }

    pub fn id(self) -> &'static str {
        crate::theme::definition(self).id
    }

    pub fn is_light(self) -> bool {
        crate::theme::definition(self).is_light
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultAgent {
    #[default]
    Automatic,
    Unassigned,
    Kind(&'static str),
}

impl DefaultAgent {
    pub fn id(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Unassigned => "unassigned",
            Self::Kind(kind) => kind,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "automatic" => Some(Self::Automatic),
            "unassigned" => Some(Self::Unassigned),
            kind => crate::beads::HARNESSES
                .iter()
                .find(|known| **known == kind)
                .copied()
                .map(Self::Kind),
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Automatic => 0,
            Self::Unassigned => 1,
            Self::Kind(kind) => crate::beads::HARNESSES
                .iter()
                .position(|known| *known == kind)
                .map_or(0, |index| index + 2),
        }
    }

    pub fn at(index: usize) -> Self {
        match index {
            0 => Self::Automatic,
            1 => Self::Unassigned,
            _ => crate::beads::HARNESSES
                .get(index - 2)
                .copied()
                .map_or(Self::Automatic, Self::Kind),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Unassigned => "Unassigned",
            Self::Kind(kind) => kind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub theme: Theme,
    pub default_agent: DefaultAgent,
    pub agent_limit: AgentLimit,
    pub auto_merge: bool,
    pub auto_merge_children: bool,
    pub auto_close_workspace: bool,
    pub auto_remove_checkout: bool,
    pub animate_drawers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            default_agent: DefaultAgent::default(),
            agent_limit: AgentLimit::default(),
            auto_merge: false,
            auto_merge_children: false,
            auto_close_workspace: false,
            auto_remove_checkout: false,
            animate_drawers: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLimit {
    Unlimited,
    Limited(u32),
}

impl Default for AgentLimit {
    fn default() -> Self {
        Self::Limited(5)
    }
}

impl AgentLimit {
    pub fn label(self) -> String {
        match self {
            Self::Unlimited => "Unlimited".to_owned(),
            Self::Limited(count) => count.to_string(),
        }
    }

    pub fn step(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Unlimited, true) => Self::Limited(1),
            (Self::Unlimited, false) | (Self::Limited(1), false) => Self::Unlimited,
            (Self::Limited(count), true) => Self::Limited(count.saturating_add(1)),
            (Self::Limited(count), false) => Self::Limited(count - 1),
        }
    }

    pub fn allows(self, active: usize) -> bool {
        match self {
            Self::Unlimited => true,
            Self::Limited(count) => active < count as usize,
        }
    }
}

/// Resolve the user configuration location without using the current workspace.
pub fn config_path() -> io::Result<PathBuf> {
    config_path_from(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))
}

fn config_path_from(xdg: Option<OsString>, home: Option<OsString>) -> io::Result<PathBuf> {
    let base = xdg.map(PathBuf::from).filter(|path| path.is_absolute());
    let base = base.or_else(|| {
        home.map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .map(|path| path.join(".config"))
    });
    base.map(|path| path.join("ontrack/settings.json"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot locate OnTrack settings: HOME is unavailable or not absolute. Set HOME or XDG_CONFIG_HOME to an absolute path.",
            )
        })
}

/// Missing settings use the default theme; invalid or unreadable files are errors.
pub fn load_from(path: &Path) -> io::Result<Settings> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(io_error("read", path, error)),
    };
    let invalid = |reason: &str| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Invalid settings in '{}': {reason}. Fix or remove the file to use defaults.",
                path.display()
            ),
        )
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| invalid(&error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("expected a JSON object"))?;
    let theme = match object.get("theme") {
        None => Theme::default(),
        Some(serde_json::Value::String(theme)) => crate::theme::find(theme)
            .ok_or_else(|| invalid(&format!("unknown theme id '{theme}'")))?,
        Some(_) => return Err(invalid("theme must be a supported string ID")),
    };
    let default_agent = match object.get("default_agent") {
        None => DefaultAgent::default(),
        Some(serde_json::Value::String(id)) => DefaultAgent::from_id(id)
            .ok_or_else(|| invalid(&format!("unknown default agent id '{id}'")))?,
        Some(_) => return Err(invalid("default_agent must be a supported string ID")),
    };
    let boolean = |key: &str, default: bool| -> io::Result<bool> {
        match object.get(key) {
            None => Ok(default),
            Some(serde_json::Value::Bool(value)) => Ok(*value),
            Some(_) => Err(invalid(&format!("{key} must be a boolean"))),
        }
    };
    let agent_limit = match object.get("agent_limit") {
        None => AgentLimit::default(),
        Some(serde_json::Value::String(value)) if value == "unlimited" => AgentLimit::Unlimited,
        Some(serde_json::Value::Number(value)) => value
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
            .filter(|count| *count > 0)
            .map(AgentLimit::Limited)
            .ok_or_else(|| invalid("agent_limit must be a positive integer or 'unlimited'"))?,
        Some(_) => {
            return Err(invalid(
                "agent_limit must be a positive integer or 'unlimited'",
            ));
        }
    };
    Ok(Settings {
        theme,
        default_agent,
        agent_limit,
        auto_merge: boolean("auto_merge", false)?,
        auto_merge_children: boolean("auto_merge_children", false)?,
        auto_close_workspace: boolean("auto_close_workspace", false)?,
        auto_remove_checkout: boolean("auto_remove_checkout", false)?,
        animate_drawers: boolean("animate_drawers", true)?,
    })
}

/// Atomically replace settings using a uniquely created temporary sibling file.
pub fn save_to(path: &Path, settings: &Settings) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Settings path must name a file",
        ));
    }
    fs::create_dir_all(parent).map_err(|error| io_error("create directory", parent, error))?;

    let (temporary_path, mut file) = create_temporary_file(parent)?;
    let theme = settings.theme.id();
    let default_agent = settings.default_agent.id();
    let agent_limit = match settings.agent_limit {
        AgentLimit::Unlimited => "\"unlimited\"".to_owned(),
        AgentLimit::Limited(count) => count.to_string(),
    };
    let result = (|| {
        writeln!(file, "{{\"theme\":\"{theme}\",\"default_agent\":\"{default_agent}\",\"agent_limit\":{agent_limit},\"auto_merge\":{},\"auto_merge_children\":{},\"auto_close_workspace\":{},\"auto_remove_checkout\":{},\"animate_drawers\":{}}}", settings.auto_merge, settings.auto_merge_children, settings.auto_close_workspace, settings.auto_remove_checkout, settings.animate_drawers)
            .and_then(|()| file.sync_all())
            .map_err(|error| io_error("write", &temporary_path, error))?;
        drop(file);
        fs::rename(&temporary_path, path).map_err(|error| io_error("replace", path, error))
    })();
    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_file(&temporary_path)
            && cleanup_error.kind() != io::ErrorKind::NotFound
        {
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; also failed to remove temporary file '{}': {cleanup_error}",
                    temporary_path.display()
                ),
            ));
        }
        return Err(error);
    }
    Ok(())
}

fn create_temporary_file(parent: &Path) -> io::Result<(PathBuf, fs::File)> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    for _ in 0..128 {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".settings-{}-{id}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("create temporary file", &path, error)),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "Cannot create a unique temporary settings file in '{}': remove stale .settings-*.tmp files and retry.",
            parent.display()
        ),
    ))
}

fn io_error(action: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "Cannot {action} '{}': {error}. Check the path and its permissions.",
            path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            loop {
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir()
                    .join(format!("ontrack-settings-test-{}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create test directory: {error}"),
                }
            }
        }

        fn path(&self) -> PathBuf {
            self.0.join("ontrack/settings.json")
        }

        fn write(&self, contents: &str) {
            let path = self.path();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_returns_default_without_creating_directories() {
        let directory = TestDirectory::new();
        assert_eq!(load_from(&directory.path()).unwrap(), Settings::default());
        assert_eq!(Settings::default().theme, Theme::Dark);
        assert!(!directory.path().parent().unwrap().exists());
    }

    #[test]
    fn every_theme_roundtrips_and_replaces_existing_settings() {
        let directory = TestDirectory::new();
        for theme in Theme::ALL.into_iter().chain([Theme::Dark]) {
            let settings = Settings {
                theme,
                ..Settings::default()
            };
            save_to(&directory.path(), &settings).unwrap();
            assert_eq!(load_from(&directory.path()).unwrap(), settings);
            let json: serde_json::Value =
                serde_json::from_slice(&fs::read(directory.path()).unwrap()).unwrap();
            assert_eq!(json["theme"], theme.id());
            assert_eq!(
                fs::read_dir(directory.path().parent().unwrap())
                    .unwrap()
                    .count(),
                1
            );
        }
    }

    #[test]
    fn missing_theme_and_unknown_fields_are_supported() {
        let directory = TestDirectory::new();
        for contents in ["{}", r#"{"future":{"enabled":true}}"#] {
            directory.write(contents);
            assert_eq!(load_from(&directory.path()).unwrap(), Settings::default());
        }
        directory.write(r#"{"theme":"light","future":[1,2,3]}"#);
        assert_eq!(load_from(&directory.path()).unwrap().theme, Theme::Light);
    }

    #[test]
    fn legacy_theme_files_default_worktree_preferences_and_new_values_roundtrip() {
        let directory = TestDirectory::new();
        directory.write(r#"{"theme":"light"}"#);
        assert_eq!(
            load_from(&directory.path()).unwrap(),
            Settings {
                theme: Theme::Light,
                ..Settings::default()
            }
        );
        assert!(!load_from(&directory.path()).unwrap().auto_merge_children);
        let settings = Settings {
            theme: Theme::Nord,
            default_agent: DefaultAgent::Kind("codex"),
            agent_limit: AgentLimit::Unlimited,
            auto_merge: true,
            auto_merge_children: false,
            auto_close_workspace: true,
            auto_remove_checkout: true,
            animate_drawers: false,
        };
        save_to(&directory.path(), &settings).unwrap();
        assert_eq!(load_from(&directory.path()).unwrap(), settings);
    }

    #[test]
    fn default_agent_roundtrips_and_rejects_unknown_ids() {
        let directory = TestDirectory::new();
        for agent in [
            DefaultAgent::Automatic,
            DefaultAgent::Unassigned,
            DefaultAgent::Kind("hermes"),
        ] {
            let settings = Settings {
                default_agent: agent,
                ..Settings::default()
            };
            save_to(&directory.path(), &settings).unwrap();
            assert_eq!(load_from(&directory.path()).unwrap(), settings);
        }
        for contents in [
            r#"{"default_agent":"unknown"}"#,
            r#"{"default_agent":null}"#,
        ] {
            directory.write(contents);
            assert_eq!(
                load_from(&directory.path()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        directory.write(r#"{"theme":"light"}"#);
        assert_eq!(
            load_from(&directory.path()).unwrap().default_agent,
            DefaultAgent::Automatic
        );
    }

    #[test]
    fn agent_limit_orders_choices_and_roundtrips() {
        let directory = TestDirectory::new();
        assert_eq!(Settings::default().agent_limit, AgentLimit::Limited(5));
        let mut choice = AgentLimit::Unlimited;
        for count in 1..=12 {
            choice = choice.step(true);
            assert_eq!(choice, AgentLimit::Limited(count));
            let settings = Settings {
                agent_limit: choice,
                ..Settings::default()
            };
            save_to(&directory.path(), &settings).unwrap();
            assert_eq!(load_from(&directory.path()).unwrap(), settings);
        }
        assert_eq!(choice.step(false), AgentLimit::Limited(11));
        assert_eq!(AgentLimit::Limited(1).step(false), AgentLimit::Unlimited);
        let settings = Settings {
            agent_limit: AgentLimit::Unlimited,
            ..Settings::default()
        };
        save_to(&directory.path(), &settings).unwrap();
        assert_eq!(load_from(&directory.path()).unwrap(), settings);
        assert!(AgentLimit::Unlimited.allows(usize::MAX));
        assert!(AgentLimit::Limited(5).allows(4));
        assert!(!AgentLimit::Limited(5).allows(5));
    }

    #[test]
    fn invalid_agent_limits_report_settings_error() {
        let directory = TestDirectory::new();
        for value in ["0", "-1", "1.5", "null", "true", "\"5\"", "4294967296"] {
            directory.write(&format!("{{\"agent_limit\":{value}}}"));
            let error = load_from(&directory.path()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("agent_limit"));
        }
    }

    #[test]
    fn child_auto_merge_defaults_off_and_rejects_non_boolean_values() {
        let directory = TestDirectory::new();
        assert!(!load_from(&directory.path()).unwrap().auto_merge_children);
        for value in ["null", "0", "\"false\""] {
            directory.write(&format!("{{\"auto_merge_children\":{value}}}"));
            let error = load_from(&directory.path()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("auto_merge_children"));
        }
        directory.write(r#"{"auto_merge_children":false}"#);
        assert!(!load_from(&directory.path()).unwrap().auto_merge_children);
    }

    #[test]
    fn malformed_json_and_invalid_themes_report_the_path_and_remedy() {
        let directory = TestDirectory::new();
        for contents in [
            "{",
            "[]",
            "null",
            r#"{"theme":"blue"}"#,
            r#"{"theme":"Dark"}"#,
            r#"{"theme":null}"#,
            r#"{"theme":42}"#,
        ] {
            directory.write(contents);
            let error = load_from(&directory.path()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            let error = error.to_string();
            assert!(error.contains(&directory.path().display().to_string()));
            assert!(error.contains("Fix or remove"));
        }
    }

    #[test]
    fn unreadable_file_reports_io_error() {
        let directory = TestDirectory::new();
        fs::create_dir_all(directory.path()).unwrap();
        let error = load_from(&directory.path()).unwrap_err().to_string();
        assert!(error.contains("Cannot read"));
        assert!(error.contains(&directory.path().display().to_string()));
    }

    #[test]
    fn parent_file_prevents_saving_and_is_preserved() {
        let directory = TestDirectory::new();
        let parent = directory.path().parent().unwrap().to_path_buf();
        fs::write(&parent, "keep me").unwrap();
        let error = save_to(&directory.path(), &Settings::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("Cannot create directory"));
        assert_eq!(fs::read_to_string(&parent).unwrap(), "keep me");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn failed_replace_cleans_temporary_file_and_preserves_destination() {
        let directory = TestDirectory::new();
        fs::create_dir_all(directory.path()).unwrap();
        let marker = directory.path().join("keep");
        fs::write(&marker, "keep me").unwrap();
        let error = save_to(&directory.path(), &Settings::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("Cannot replace"));
        assert_eq!(fs::read_to_string(marker).unwrap(), "keep me");
        assert_eq!(
            fs::read_dir(directory.path().parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn absolute_xdg_path_takes_precedence_over_home() {
        assert_eq!(
            config_path_from(Some("/xdg".into()), Some("/example-home".into())).unwrap(),
            PathBuf::from("/xdg/ontrack/settings.json")
        );
        assert_eq!(
            config_path_from(Some("/xdg".into()), None).unwrap(),
            PathBuf::from("/xdg/ontrack/settings.json")
        );
    }

    #[test]
    fn absent_empty_or_relative_xdg_uses_absolute_home() {
        for xdg in [None, Some("".into()), Some("relative".into())] {
            assert_eq!(
                config_path_from(xdg, Some("/example-home".into())).unwrap(),
                PathBuf::from("/example-home/.config/ontrack/settings.json")
            );
        }
    }

    #[test]
    fn unavailable_home_without_absolute_xdg_is_an_error() {
        for xdg in [None, Some("".into()), Some("relative".into())] {
            for home in [None, Some("".into())] {
                let error = config_path_from(xdg.clone(), home).unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::NotFound);
                assert!(error.to_string().contains("HOME is unavailable"));
            }
        }
    }

    #[test]
    fn relative_home_without_absolute_xdg_is_rejected() {
        for xdg in [None, Some("".into()), Some("relative".into())] {
            let error = config_path_from(xdg, Some("relative/home".into())).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
            assert!(error.to_string().contains("absolute path"));
        }
    }
}
