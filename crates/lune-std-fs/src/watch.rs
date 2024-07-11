use std::{default::Default, time::Duration};

use mlua::prelude::*;
use notify::{Config, Event, RecommendedWatcher, Watcher};

pub struct WatchOptions {
    /// A glob pattern defining which files to watch.
    pub pattern: String,
    /// Whether to watch changes recursively.
    pub recursive: bool,
    /// Whether to watch files.
    pub watch_files: bool,
    /// Whether to watch directories.
    pub watch_diretories: bool,
    /// The interval in seconds to poll for changes.
    pub interval: Option<u64>,
}

impl WatchOptions {
    pub fn into_watcher(
        self,
        tx: tokio::sync::mpsc::Sender<notify::Result<Event>>,
    ) -> notify::Result<RecommendedWatcher> {
        RecommendedWatcher::new(
            move |res| tx.blocking_send(res).unwrap(),
            Config::default().with_poll_interval(Duration::from_secs(self.interval.unwrap_or(30))),
        )
    }
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            pattern: String::default(),
            recursive: false,
            watch_files: true,
            watch_diretories: true,
            interval: Some(30),
        }
    }
}

impl FromLua<'_> for WatchOptions {
    fn from_lua(value: LuaValue<'_>, _: &'_ mlua::Lua) -> LuaResult<Self> {
        match value {
            LuaValue::String(s) => Ok(Self {
                pattern: s.to_str()?.to_string(),
                ..Self::default()
            }),
            LuaValue::Table(t) => Ok(Self {
                pattern: t.get("pattern")?,
                recursive: t.get("recursive").unwrap_or_default(),
                watch_files: t.get("watchFiles").unwrap_or_default(),
                watch_diretories: t.get("watchDirectories").unwrap_or_default(),
                interval: t.get("interval").unwrap_or_default(),
            }),
            other => Err(LuaError::FromLuaConversionError {
                from: other.type_name(),
                to: "WatchOptions",
                message: Some("Argument must be of type string or table".to_string()),
            }),
        }
    }
}
