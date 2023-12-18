use std::{fs::read_dir, time::Instant,ffi::OsString, thread::{spawn, JoinHandle}};
use pkgbuild::{self, Pkgbuild};

fn main() {
    let mut list = Vec::new();
    for entry in read_dir(".").unwrap() {
        let entry = entry.unwrap();
        list.push(entry.file_name())
    }
    list.sort_unstable();
    println!("Count of PKGBUILDs: {}", list.len());
    println!("Testing single-thread reading...");
    let mut time_start = Instant::now();
    let mut pkgbuilds = pkgbuild::parse_multi(&list).unwrap();
    println!("Single-thread reading took {} seconds", (Instant::now() - time_start).as_secs_f64());
    println!("First PKGBUILD is {}, last is {}", pkgbuilds.first().unwrap().pkgbase, pkgbuilds.last().unwrap().pkgbase);
    let mut step: usize = list.len() / std::thread::available_parallelism().unwrap();
    let mut lists: Vec<Vec<OsString>> = Vec::new();
    while ! list.is_empty() {
        let len = list.len();
        if step > len {
            step = len
        }
        lists.push(list.drain((len - step)..len).collect())
    }
    println!("Testing {}-thread reading...", lists.len());
    time_start = Instant::now();
    let threads: Vec<JoinHandle<Vec<Pkgbuild>>> = 
        lists.into_iter().map(|list|spawn(
            move ||pkgbuild::parse_multi(list).unwrap())).collect();
    let mut chunks: Vec<Vec<Pkgbuild>> = threads.into_iter().map(
        |thread|thread.join().unwrap()).collect();
    pkgbuilds.clear();
    while let Some(mut chunk) = chunks.pop() {
        pkgbuilds.append(&mut chunk);
    }
    println!("Multi-thread reading took {} seconds", (Instant::now() - time_start).as_secs_f64());
    println!("First PKGBUILD is {}, last is {}", pkgbuilds.first().unwrap().pkgbase, pkgbuilds.last().unwrap().pkgbase);
}