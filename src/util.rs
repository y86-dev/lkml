use std::{
    fs,
    io::{self, Read},
    path::Path,
};

/// Add a total count to any reader.
pub struct ReadCounter<R> {
    reader: R,
    count: u64,
}

impl<R> ReadCounter<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, count: 0 }
    }

    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    pub fn count(&self) -> u64 {
        self.count
    }
}

impl<R: Read> Read for ReadCounter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        self.count += u64::try_from(n).unwrap();
        Ok(n)
    }
}

/// Try to create a directory, ignore already exists errors.
pub fn create_dir_if_not_exists(path: impl AsRef<Path>) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}
