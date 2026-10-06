//! Migration paths remain relative to held directory descriptors after selection.
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

pub(super) struct AnchoredPath {
    pub parent: File,
    pub leaf: OsString,
    pub display: PathBuf,
}

fn io_error(error: rustix::io::Errno) -> io::Error {
    error.into()
}

fn trusted_start(path: &Path) -> io::Result<(File, PathBuf)> {
    if !path.is_absolute() {
        return Ok((
            fs::open(".", DIR_FLAGS, Mode::empty())
                .map(File::from)
                .map_err(io_error)?,
            path.to_owned(),
        ));
    }
    // Canonicalization is confined to operator-selected repo/HOME anchors.
    // Descendants, including .cx, are always walked with NOFOLLOW.
    let anchors = [crate::paths::repo_root(), crate::paths::home_dir()];
    for anchor in anchors.into_iter().flatten() {
        if anchor.is_absolute()
            && let Ok(tail) = path.strip_prefix(&anchor)
        {
            let selected = std::fs::canonicalize(&anchor)?;
            let dir = fs::open(&selected, DIR_FLAGS, Mode::empty())
                .map(File::from)
                .map_err(io_error)?;
            return Ok((dir, tail.to_owned()));
        }
    }
    #[cfg(target_os = "macos")]
    for (alias, selected) in [("/tmp", "/private/tmp"), ("/var", "/private/var")] {
        if let Ok(tail) = path.strip_prefix(alias) {
            let dir = fs::open(selected, DIR_FLAGS, Mode::empty())
                .map(File::from)
                .map_err(io_error)?;
            return Ok((dir, tail.to_owned()));
        }
    }
    Ok((
        fs::open("/", DIR_FLAGS, Mode::empty())
            .map(File::from)
            .map_err(io_error)?,
        path.strip_prefix("/").unwrap().to_owned(),
    ))
}

impl AnchoredPath {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        let leaf = path
            .file_name()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "migration path must name a file",
                )
            })?
            .to_owned();
        let (mut parent, tail) = trusted_start(path)?;
        let tail_parent = tail.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration path cannot be an anchor",
            )
        })?;
        for component in tail_parent.components() {
            let name = match component {
                Component::Normal(name) => name,
                Component::ParentDir => OsStr::new(".."),
                Component::CurDir => continue,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "unexpected migration path component",
                    ));
                }
            };
            match fs::openat(&parent, name, DIR_FLAGS, Mode::empty()) {
                Ok(fd) => parent = File::from(fd),
                Err(rustix::io::Errno::NOENT) if create => {
                    match fs::mkdirat(&parent, name, Mode::from_raw_mode(0o700)) {
                        Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                        Err(error) => return Err(io_error(error)),
                    }
                    parent = File::from(
                        fs::openat(&parent, name, DIR_FLAGS, Mode::empty()).map_err(io_error)?,
                    );
                }
                Err(error) => return Err(io_error(error)),
            }
        }
        Ok(Self {
            parent,
            leaf,
            display: path.to_owned(),
        })
    }

    pub fn source(&self) -> io::Result<File> {
        let file = File::from(
            fs::openat(
                &self.parent,
                &self.leaf,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(io_error)?,
        );
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration source is not a regular file",
            ));
        }
        Ok(file)
    }

    pub fn check_target(&self) -> io::Result<()> {
        match fs::statat(&self.parent, &self.leaf, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile => Ok(()),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration destination is not a regular file",
            )),
            Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    pub fn aliases(&self, other: &Self, source: &File) -> io::Result<bool> {
        let a = self.parent.metadata()?;
        let b = other.parent.metadata()?;
        if a.dev() == b.dev() && a.ino() == b.ino() && self.leaf == other.leaf {
            return Ok(true);
        }
        match fs::statat(&self.parent, &self.leaf, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => {
                let original = fs::fstat(source).map_err(io_error)?;
                Ok(stat.st_dev == original.st_dev && stat.st_ino == original.st_ino)
            }
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(error) => Err(io_error(error)),
        }
    }
}

pub(super) struct PrivateFile {
    pub file: File,
    parent: File,
    name: OsString,
    pub display: PathBuf,
    retained: bool,
    container: Option<(File, OsString)>,
}

impl PrivateFile {
    pub fn create(path: &AnchoredPath, backup: bool) -> io::Result<Self> {
        for _ in 0..32 {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            // A valid NAME_MAX source/output leaf must leave room for private names.
            let mut name = OsString::from(".cx-log");
            let suffix = if backup {
                format!(
                    ".bak.{}.{}.{}",
                    chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
                    std::process::id(),
                    id
                )
            } else {
                format!(".migrate.{}.{}.tmp", std::process::id(), id)
            };
            name.push(suffix);
            match if backup {
                Self::named(path, name)
            } else {
                Self::staged(path, name)
            } {
                Ok(file) => return Ok(file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "migration private-file collisions exhausted",
        ))
    }

    fn named(path: &AnchoredPath, name: OsString) -> io::Result<Self> {
        let parent = path.parent.try_clone()?;
        let file = File::from(
            fs::openat(
                &parent,
                &name,
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(io_error)?,
        );
        let display = path.display.with_file_name(&name);
        Ok(Self {
            file,
            parent,
            name,
            display,
            retained: false,
            container: None,
        })
    }

    fn staged(path: &AnchoredPath, name: OsString) -> io::Result<Self> {
        let outer = path.parent.try_clone()?;
        fs::mkdirat(&outer, &name, Mode::from_raw_mode(0o700)).map_err(io_error)?;
        let parent = match fs::openat(&outer, &name, DIR_FLAGS, Mode::empty()) {
            Ok(fd) => File::from(fd),
            Err(error) => {
                let _ = fs::unlinkat(&outer, &name, AtFlags::REMOVEDIR);
                return Err(io_error(error));
            }
        };
        let staged = AnchoredPath {
            parent,
            leaf: OsString::from("data"),
            display: path.display.with_file_name(&name).join("data"),
        };
        let result = Self::named(&staged, OsString::from("data"));
        match result {
            Ok(mut file) => {
                file.container = Some((outer, name));
                let directory = file.parent.metadata()?;
                let contents = file.file.metadata()?;
                if directory.uid() != contents.uid() || directory.mode() & 0o777 != 0o700 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "migration staging directory is not private",
                    ));
                }
                Ok(file)
            }
            Err(error) => {
                let _ = fs::unlinkat(&outer, &name, AtFlags::REMOVEDIR);
                Err(error)
            }
        }
    }

    pub fn retain(&mut self) {
        self.retained = true;
    }
    pub fn sync_parent(&self) -> io::Result<()> {
        self.parent.sync_all()
    }

    pub fn publish(&mut self, target: &AnchoredPath) -> io::Result<()> {
        target.check_target()?;
        // renameat replaces a directory entry; it never follows a destination symlink.
        fs::renameat(&self.parent, &self.name, &target.parent, &target.leaf).map_err(io_error)?;
        self.retained = true;
        Ok(())
    }
}

impl Drop for PrivateFile {
    fn drop(&mut self) {
        if !self.retained {
            // Remove only the entry that still names our opened file.
            if let (Ok(stat), Ok(meta)) = (
                fs::statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW),
                fs::fstat(&self.file),
            ) && stat.st_dev == meta.st_dev
                && stat.st_ino == meta.st_ino
            {
                let _ = fs::unlinkat(&self.parent, &self.name, AtFlags::empty());
            }
        }
        if let Some((outer, name)) = &self.container
            && let (Ok(entry), Ok(held)) = (
                fs::statat(outer, name, AtFlags::SYMLINK_NOFOLLOW),
                fs::fstat(&self.parent),
            )
            && entry.st_dev == held.st_dev
            && entry.st_ino == held.st_ino
        {
            let _ = fs::unlinkat(outer, name, AtFlags::REMOVEDIR);
        }
    }
}

#[cfg(test)]
#[path = "logs_fs_tests.rs"]
mod tests;
