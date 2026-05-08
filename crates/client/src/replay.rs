use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use sim::{DEFAULT_SEED, Input};

pub struct Replay {
    pub path: PathBuf,
    file: File,
    pub seed: u64,
    pub recorded: Vec<[Input; 2]>,
}

impl Replay {
    pub fn open() -> Self {
        let path = replay_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let (seed, recorded) = match File::open(&path) {
            Ok(mut f) => {
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).expect("read replay.bin");
                if buf.len() >= 8 {
                    let seed = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                    let recorded = buf[8..]
                        .chunks_exact(2)
                        .map(|c| [Input::from_bits_truncate(c[0]), Input::from_bits_truncate(c[1])])
                        .collect();
                    (seed, recorded)
                } else {
                    (DEFAULT_SEED, Vec::new())
                }
            }
            Err(_) => (DEFAULT_SEED, Vec::new()),
        };

        if recorded.is_empty() {
            let mut f = File::create(&path).expect("create replay.bin");
            f.write_all(&seed.to_le_bytes()).expect("write seed");
        }

        let file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open replay.bin for append");

        Self { path, file, seed, recorded }
    }

    pub fn record(&mut self, inputs: [Input; 2]) {
        let _ = self.file.write_all(&[inputs[0].bits(), inputs[1].bits()]);
    }

    pub fn reset(&mut self) {
        let mut f = File::create(&self.path).expect("truncate replay.bin");
        f.write_all(&DEFAULT_SEED.to_le_bytes()).expect("write seed");
        self.file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .expect("reopen replay.bin");
        self.seed = DEFAULT_SEED;
        self.recorded.clear();
        println!("[replay] reset");
    }
}

fn replay_path() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| p.parent())
        .map(|target| target.join("replay.bin"))
        .unwrap_or_else(|| PathBuf::from("target/replay.bin"))
}
