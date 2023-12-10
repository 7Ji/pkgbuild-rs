use std::{env::ArgsOs, os::unix::{ffi::OsStrExt, process::CommandExt}, path::PathBuf, io::{stdout, Write}};

use pkgbuild::Pkgbuild;

fn applet_spawner(args: &mut ArgsOs) {
    let output = std::process::Command::new("/proc/self/exe")
        .arg0("child")
        .args(args)
        .output()
        .expect("Failed to spawn child and collect output");
    if ! output.status.success() {
        eprintln!("Child not success:\n>>>>\ncode: {:?}\nstdout:\n{}\nstderr:\n{}\n<<<<", 
            output.status.code(), 
            String::from_utf8_lossy(&output.stdout), 
            String::from_utf8_lossy(&output.stderr));
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

fn applet_child(args: &mut ArgsOs) {
    let pkgbuilds = pkgbuild::parse_multi(args).expect("Failed to parse PKGBUIlDs");
    let data = rmp_serde::to_vec(&pkgbuilds).expect("Failed to serialize");
    stdout().write_all(&data).expect("Failed to write serialized PKGBUILDs to stdout");
}

fn dispatch(args: &mut ArgsOs) {
    let applet = PathBuf::from(args.next().expect("Failed to dispatch: no more arg0"));
    let applet = applet.file_name().unwrap_or(applet.as_ref());
    match applet.as_bytes() {
        // main
        b"multi" => dispatch(args),
        b"spawner" => applet_spawner(args),
        b"child" => applet_child(args),
        _ => panic!("unknown applet {:?}", applet),
    }
}

fn main() {
    dispatch(&mut std::env::args_os())
}