use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

struct Split {
    pub name: String,
    pub duration: Duration,
}

impl Split {
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        write!(writer, "{}: {:?}", self.name, self.duration)?;
        Ok(())
    }
}

pub struct Stopwatch {
    pub splits: Vec<Split>,
    pub name: String,
    pub start_total: Instant,
    pub start_split: Instant,
}

impl Stopwatch {
    pub fn new(name: String) -> Stopwatch {
        let now = Instant::now();
        Stopwatch {
            splits: Vec::default(),
            name,
            start_total: now,
            start_split: now,
        }
    }

    pub fn total_elapsed(&self) -> Duration {
        self.start_total.elapsed()
    }
    pub fn split_elapsed(&self) -> Duration {
        self.start_split.elapsed()
    }

    pub fn record_split(&mut self, name: String) -> Duration {
        let duration = self.split_elapsed();
        self.start_split = Instant::now();
        self.splits.push(Split { name, duration });
        duration
    }

    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(
            writer,
            "Stopwatch: {};  Total elapsed duration: {:?}",
            self.name,
            self.total_elapsed()
        )?;
        for split in self.splits.iter() {
            write!(writer, "  ");
            split.display(writer);
            writeln!(writer);
        }
        Ok(())
    }
}
