use std::path::PathBuf;


/// Options used by `ParserScript` when parsing `PKGBUILD`s
pub struct ParserOptions {
    /// The interpreter used for the parser script, changing this only makes
    /// sense if you're working with a non-standard installation
    /// 
    /// Default: `/bin/bash`
    pub intepreter: PathBuf,

    /// Change the working directory before calling interpreter with the script
    /// 
    /// Default: `None`
    pub work_dir: Option<PathBuf>,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            intepreter: "/bin/bash".into(),
            work_dir: None
        }
    }
}

impl ParserOptions {
    /// Get a `ParserOptions` instance with default settings: no network, does
    /// not change work_dir
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the interpreter used for the `ParserScript`
    pub fn set_interpreter<P: Into<PathBuf>>(&mut self, interpreter: P)
    -> &mut Self
    {
        self.intepreter = interpreter.into();
        self
    }

    /// Set the work_dir to change to for the parser
    pub fn set_work_dir<P: Into<PathBuf>>(&mut self, work_dir: Option<P>)
    -> &mut Self
    {
        self.work_dir = work_dir.map(|path|path.into());
        self
    }
}