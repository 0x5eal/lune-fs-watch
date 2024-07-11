#![allow(clippy::cargo_common_metadata)]

use std::io::ErrorKind as IoErrorKind;
use std::path::PathBuf;

use bstr::{BString, ByteSlice};
use globset::Glob;
use notify::event::AccessKind;
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::fs;

use mlua::prelude::*;
use mlua_luau_scheduler::LuaSchedulerExt;

use lune_utils::TableBuilder;
use watch::WatchOptions;

mod copy;
mod metadata;
mod options;
mod watch;

use self::copy::copy;
use self::metadata::FsMetadata;
use self::options::FsWriteOptions;

/**
    Creates the `fs` standard library module.

    # Errors

    Errors when out of memory.
*/
pub fn module(lua: &Lua) -> LuaResult<LuaTable> {
    TableBuilder::new(lua)?
        .with_async_function("readFile", fs_read_file)?
        .with_async_function("readDir", fs_read_dir)?
        .with_async_function("writeFile", fs_write_file)?
        .with_async_function("writeDir", fs_write_dir)?
        .with_async_function("removeFile", fs_remove_file)?
        .with_async_function("removeDir", fs_remove_dir)?
        .with_async_function("metadata", fs_metadata)?
        .with_async_function("isFile", fs_is_file)?
        .with_async_function("isDir", fs_is_dir)?
        .with_async_function("move", fs_move)?
        .with_async_function("copy", fs_copy)?
        .with_async_function("watch", fs_watch)?
        .build_readonly()
}

async fn fs_read_file(lua: &Lua, path: String) -> LuaResult<LuaString> {
    let bytes = fs::read(&path).await.into_lua_err()?;

    lua.create_string(bytes)
}

async fn fs_read_dir(_: &Lua, path: String) -> LuaResult<Vec<String>> {
    let mut dir_strings = Vec::new();
    let mut dir = fs::read_dir(&path).await.into_lua_err()?;
    while let Some(dir_entry) = dir.next_entry().await.into_lua_err()? {
        if let Some(dir_name_str) = dir_entry.file_name().to_str() {
            dir_strings.push(dir_name_str.to_owned());
        } else {
            return Err(LuaError::RuntimeError(format!(
                "File name could not be converted into a string: '{}'",
                dir_entry.file_name().to_string_lossy()
            )));
        }
    }
    Ok(dir_strings)
}

async fn fs_write_file(_: &Lua, (path, contents): (String, BString)) -> LuaResult<()> {
    fs::write(&path, contents.as_bytes()).await.into_lua_err()
}

async fn fs_write_dir(_: &Lua, path: String) -> LuaResult<()> {
    fs::create_dir_all(&path).await.into_lua_err()
}

async fn fs_remove_file(_: &Lua, path: String) -> LuaResult<()> {
    fs::remove_file(&path).await.into_lua_err()
}

async fn fs_remove_dir(_: &Lua, path: String) -> LuaResult<()> {
    fs::remove_dir_all(&path).await.into_lua_err()
}

async fn fs_metadata(_: &Lua, path: String) -> LuaResult<FsMetadata> {
    match fs::metadata(path).await {
        Err(e) if e.kind() == IoErrorKind::NotFound => Ok(FsMetadata::not_found()),
        Ok(meta) => Ok(FsMetadata::from(meta)),
        Err(e) => Err(e.into()),
    }
}

async fn fs_is_file(_: &Lua, path: String) -> LuaResult<bool> {
    match fs::metadata(path).await {
        Err(e) if e.kind() == IoErrorKind::NotFound => Ok(false),
        Ok(meta) => Ok(meta.is_file()),
        Err(e) => Err(e.into()),
    }
}

async fn fs_is_dir(_: &Lua, path: String) -> LuaResult<bool> {
    match fs::metadata(path).await {
        Err(e) if e.kind() == IoErrorKind::NotFound => Ok(false),
        Ok(meta) => Ok(meta.is_dir()),
        Err(e) => Err(e.into()),
    }
}

async fn fs_move(_: &Lua, (from, to, options): (String, String, FsWriteOptions)) -> LuaResult<()> {
    let path_from = PathBuf::from(from);
    if !path_from.exists() {
        return Err(LuaError::RuntimeError(format!(
            "No file or directory exists at the path '{}'",
            path_from.display()
        )));
    }
    let path_to = PathBuf::from(to);
    if !options.overwrite && path_to.exists() {
        return Err(LuaError::RuntimeError(format!(
            "A file or directory already exists at the path '{}'",
            path_to.display()
        )));
    }
    fs::rename(path_from, path_to).await.into_lua_err()?;
    Ok(())
}

async fn fs_copy(_: &Lua, (from, to, options): (String, String, FsWriteOptions)) -> LuaResult<()> {
    copy(from, to, options).await
}

async fn fs_watch(
    lua: &Lua,
    (root_path, options, handlers): (String, WatchOptions, LuaTable<'_>),
) -> LuaResult<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut watcher = options.as_watcher(tx).into_lua_err()?;

    let added_handler = handlers.get::<_, LuaFunction>("added").ok();
    let read_handler = handlers.get::<_, LuaFunction>("read").ok();
    let removed_handler = handlers.get::<_, LuaFunction>("removed").ok();
    let changed_handler = handlers.get::<_, LuaFunction>("changed").ok();

    let glob = Glob::new(&options.pattern)
        .into_lua_err()?
        .compile_matcher();

    watcher
        .watch(
            &PathBuf::from(root_path),
            if options.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            },
        )
        .into_lua_err()?;

    while let Some(res) = rx.recv().await {
        let event = res.into_lua_err()?;
        let filtered_paths = event
            .paths
            .iter()
            .filter(|elem| {
                (elem.is_file() && options.watch_files)
                    || (elem.is_dir() && options.watch_diretories)
            })
            .filter(|elem| (glob.is_match(elem)))
            .map(|elem| elem.to_string_lossy())
            .collect::<Vec<_>>();

        if filtered_paths.is_empty() {
            continue;
        }

        let handler = match event.kind {
            EventKind::Access(AccessKind::Read) => &read_handler, // File was read
            EventKind::Remove(_) => &removed_handler,             // File was removed
            EventKind::Create(_) => &added_handler,               // File was created
            EventKind::Modify(_) => &changed_handler,             // File was mutated

            // Unsupported Events
            EventKind::Any | EventKind::Other | EventKind::Access(_) => continue,
        };

        if let Some(handler) = handler {
            lua.push_thread_back(handler, filtered_paths)?;
        }
    }

    Ok(())
}
