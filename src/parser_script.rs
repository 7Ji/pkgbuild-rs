use std::{path::{PathBuf, Path}, ffi::OsStr};

use tempfile::NamedTempFile;

use crate::{error::Result, ParserScriptBuilder};

pub enum ParserScript {
    Temporary(NamedTempFile),
    Persistent(PathBuf),
}

impl AsRef<OsStr> for ParserScript {
    fn as_ref(&self) -> &OsStr {
        match self {
            ParserScript::Temporary(temp_file) => 
                temp_file.path().as_os_str(),
            ParserScript::Persistent(path) => path.as_os_str(),
        }
    }
}

impl ParserScript {
    /// Generate a parser script at the given path, or create a named tempfile
    /// to store the script. 
    /// 
    /// This uses either `LIBRARY` from env or `/usr/share/makekg` if the env
    /// is missing for `makepkg_library` (named `LIBRARY` in  `makepkg` 
    /// routines) and either `MAKEPKG_CONF` from env or `/etc/makepkg.conf` if
    /// the env is missing for `makepkg_config` (named `MAKEPKG_CONF` in 
    /// `makepkg` routines). 
    /// 
    /// If customization on those variables are needed, then caller should 
    /// create a `ParserScript` with a `ParserScriptBuilder`
    pub fn new<P: AsRef<Path>>(path: Option<P>) -> Result<Self> {
        ParserScriptBuilder::new().build(path)
    }
}