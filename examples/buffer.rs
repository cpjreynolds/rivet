extern crate rivet;

use std::io::prelude::*;
use rivet::buffer::sync;
use std::iter;
use std::thread;

fn main() {
    let (tx, rx) = sync::ring(1 << 16).unwrap();

    thread::spawn(move || {
        let buf: Vec<u8> = iter::repeat(1).take(500).collect();
        let mut nwrit = 0;
        let mut write_misses = 0;

        loop {
            match tx.try_write(&buf) {
                0 => {
                    write_misses += 1;
                    println!("write miss; nwrit = {}; total = {}", nwrit, write_misses);
                },
                n => nwrit += n,
            }
        }
    });

    let mut buf: Vec<u8> = iter::repeat(0).take(465).collect();
    let mut nread = 0;
    let mut read_misses = 0;

    loop {
        match rx.try_read(&mut buf) {
            0 => {
                read_misses += 1;
                println!("read miss; nread = {}; total = {}", nread, read_misses);
            },
            n => nread += n,
        }
    }
}
