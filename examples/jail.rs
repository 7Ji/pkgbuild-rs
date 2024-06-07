use std::{env::current_dir, ffi::OsString, fs::{read_dir, read_link, File}, io::{copy, stdout, Write}, os::unix::ffi::OsStrExt, path::{Path, PathBuf}, process::Command, thread::sleep};

use pkgbuild::Pkgbuild;
use tempfile::tempdir;

fn clone_file(path_old: &Path, path_new: &Path) {
    let mut file_in = File::open(&path_old).expect("Failed to open old PKGBUILD");
    let mut file_out = File::create_new(&path_new).expect("Failed to create new PKGBUILD");
    copy(&mut file_in, &mut file_out).expect("Failed to clone PKGBUILD");
}

fn parent() {
    let dir = tempdir().expect("Failed to create temp dir");
    for path in std::env::args_os().skip(1) {
        let path = PathBuf::from(path);
        let mut name = OsString::new();
        for component in path.components() {
            if let std::path::Component::Normal(component) = component {
                name.push(component)
            }
        }
        let path_new = dir.path().join(name);
        clone_file(&path, &path_new);
    }
    // let dir_exe = tempdir().expect("Failed to create temp dir for exe");
    let exe = read_link("/proc/self/exe").expect("Failed to read self exe");
    // Clone it to avoid 
    // let file = tempfile().expect("Failed to create temp file");
    // let path_exe = dir_exe.path().join("jail-child");
    // clone_file(&exe, &path_exe);
    let mut command = Command::new("/usr/bin/bwrap");
    command.args([
        "--unshare-all",
        "--share-net",
        "--cap-drop", "ALL",
        "--die-with-parent",
        "--ro-bind", "/usr", "/usr", 
        "--symlink", "usr/lib", "/lib",
        "--symlink", "usr/lib", "/lib64",
        "--symlink", "usr/bin", "/bin",
        "--symlink", "usr/bin", "/sbin",
        "--dev", "/dev",
        "--proc", "/proc",
        "--ro-bind", "/etc/machine-id", "/etc/machine-id",
        "--ro-bind", "/etc/passwd", "/etc/passwd",
        "--ro-bind", "/etc/nsswitch.conf", "/etc/nsswitch.conf",
        "--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf",
        "--ro-bind", "/etc/localtime", "/etc/localtime",
        "--ro-bind", "/etc/makepkg.conf", "/etc/makepkg.conf",
        "--dir", "/tmp"
    ]);
    command.arg("--ro-bind")
        .arg(&exe)
        .arg("/jail-child")
        .arg("--ro-bind")
        .arg(dir.path())
        .arg("/PKGBUILDs")
        .arg("--chdir")
        .arg("/PKGBUILDs")
        .arg("--")
        .arg("/jail-child");
    let output = command.output().expect("Failed to wait for child output");
    if ! output.status.success() {
        eprintln!("Child not success:\n>>>>\ncode: {:?}\nstdout:\n{}\nstderr:\n{}\n<<<<", 
            output.status.code(), 
            String::from_utf8_lossy(&output.stdout), 
            String::from_utf8_lossy(&output.stderr));
        sleep(std::time::Duration::from_secs(100));
        panic!("Child not success");
    }
    let pkgbuilds: Vec<Pkgbuild> = rmp_serde::from_slice(&output.stdout).expect("Failed to deserialize PKGBUILDs");
    for pkgbuild in pkgbuilds.iter() {
        #[cfg(feature = "format")]
        println!("{}", pkgbuild);
        #[cfg(not(feature = "format"))]
        println!("{:?}", pkgbuild);
    }
}

fn child() {
    let mut names = Vec::new();
    for entry in read_dir(".").expect("Failed to read dir") {
        let entry = entry.expect("Failed to read PKGBUILD name");
        let name = entry.file_name();
        if name.is_empty() {
            continue
        }
        names.push(name)
    }
    if names.is_empty() {
        return
    }
    eprintln!("PKGBUILD names: {:?}", &names);
    eprintln!("Current work directory: {}", current_dir().expect("Failed to get current dir").display());
    let pkgbuilds = pkgbuild::parse_multi(&names).expect("Failed to parse PKGBUILDs");
    let data = rmp_serde::to_vec(&pkgbuilds).expect("Failed to serialize");
    stdout().write_all(&data).expect("Failed to write serialized PKGBUILDs to stdout");
}

fn dispatch() {
    let exe = read_link("/proc/self/exe").expect("Failed to read self process path");
    if exe.as_os_str().as_bytes() == b"/jail-child" {
        child()
    } else {
        parent()
    }
}

fn main() {
    dispatch()
}